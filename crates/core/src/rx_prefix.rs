use std::collections::HashMap;

/// Mirrors the `~/.config/rx/prefixes.toml` schema.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct RxPrefixConfig {
    /// Definite mappings: command (or "cmd sub") → prefix argv.
    #[serde(default)]
    pub mappings: HashMap<String, Vec<String>>,
    /// Candidate prefixes to try when no mapping exists.
    #[serde(default)]
    pub candidate_prefixes: Vec<Vec<String>>,
    /// Whether to persist a successful candidate as a confirmed mapping.
    #[serde(default)]
    pub learn_on_successful_fallback: bool,
}

/// Port for reading and writing the rx prefix config.
pub trait PrefixStore {
    fn load(&self) -> RxPrefixConfig;
    /// Merge-write: add `key → prefix` to existing mappings without overwriting others.
    fn confirm_mapping(&self, key: &str, prefix: &[String]);
}

/// File-backed implementation reading `~/.config/rx/prefixes.toml`.
pub struct FilePrefixStore {
    pub path: std::path::PathBuf,
}

impl FilePrefixStore {
    pub fn default_path() -> std::path::PathBuf {
        std::env::var_os("CRS_RX_PREFIXES")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                let base = std::env::var_os("XDG_CONFIG_HOME")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| {
                        std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                            .join(".config")
                    });
                base.join("rx").join("prefixes.toml")
            })
    }
}

impl PrefixStore for FilePrefixStore {
    fn load(&self) -> RxPrefixConfig {
        let Ok(content) = std::fs::read_to_string(&self.path) else {
            return RxPrefixConfig::default();
        };
        toml::from_str(&content).unwrap_or_default()
    }

    fn confirm_mapping(&self, key: &str, prefix: &[String]) {
        let mut config = self.load();
        config.mappings.insert(key.to_string(), prefix.to_vec());
        match toml::to_string_pretty(&config) {
            Ok(serialized) => {
                if let Err(e) = std::fs::write(&self.path, &serialized) {
                    eprintln!("crs: warn: could not write rx prefixes to {}: {e}", self.path.display());
                }
            }
            Err(e) => {
                eprintln!("crs: warn: could not serialize rx prefixes: {e}");
            }
        }
    }
}

/// Test double — injected config, records what was written.
#[cfg(test)]
pub struct FakePrefixStore {
    pub config: RxPrefixConfig,
    pub written: std::cell::RefCell<Option<(String, Vec<String>)>>,
}

#[cfg(test)]
impl PrefixStore for FakePrefixStore {
    fn load(&self) -> RxPrefixConfig {
        self.config.clone()
    }
    fn confirm_mapping(&self, key: &str, prefix: &[String]) {
        *self.written.borrow_mut() = Some((key.to_string(), prefix.to_vec()));
    }
}

/// One shell segment plus the separator that followed it (if any).
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// The raw text of this segment (may have leading/trailing spaces).
    pub text: String,
    /// The separator token that terminated this segment: `&&`, `||`, `;`, or `|`.
    pub sep: Option<String>,
}

/// Split a shell command string into segments on `&&`, `||`, `;`, `|`.
///
/// Preserves surrounding whitespace in each segment so `rejoin` is lossless.
/// Does NOT handle quotes — splitting is purely textual, which is correct for
/// the commands we see in practice (no quoted separators).
pub fn split_segments(cmd: &str) -> Vec<Segment> {
    // Separators in priority order (longest first so `||` beats `|`).
    let seps = ["&&", "||", ";", "|"];
    let mut result = Vec::new();
    let mut remaining = cmd;

    'outer: loop {
        // Find the earliest separator in remaining.
        let mut earliest: Option<(usize, &str)> = None;
        for sep in &seps {
            if let Some(pos) = remaining.find(sep)
                && earliest.is_none_or(|(e, _)| pos < e)
            {
                earliest = Some((pos, sep));
            }
        }
        match earliest {
            None => {
                result.push(Segment { text: remaining.to_string(), sep: None });
                break 'outer;
            }
            Some((pos, sep)) => {
                result.push(Segment {
                    text: remaining[..pos].to_string(),
                    sep: Some(sep.to_string()),
                });
                remaining = &remaining[pos + sep.len()..];
            }
        }
    }
    result
}

/// Reconstruct the original command string from segments.
pub fn rejoin(segs: &[Segment]) -> String {
    let mut out = String::new();
    for seg in segs {
        out.push_str(&seg.text);
        if let Some(sep) = &seg.sep {
            out.push_str(sep);
        }
    }
    out
}

/// Result of a prefix lookup for a single segment's base command.
#[derive(Debug, Clone, PartialEq)]
pub enum PrefixMatch {
    /// A definite mapping exists in `mappings`.
    Confirmed { key: String, prefix: Vec<String> },
    /// No mapping; a candidate prefix is being tried speculatively.
    Candidate { key: String, prefix: Vec<String> },
}

/// Look up the prefix for the leading command word(s) of `segment`.
///
/// Returns `None` if:
/// - `segment` contains `$(` or a backtick (subshell — unsafe to rewrite blindly)
/// - no mapping or candidate applies
///
/// Two-word key check happens before single-word.
pub fn lookup_prefix(segment: &str, config: &RxPrefixConfig) -> Option<PrefixMatch> {
    let trimmed = segment.trim();
    if trimmed.contains("$(") || trimmed.contains('`') {
        return None;
    }

    let tokens = shell_words::split(trimmed).ok()?;
    let first = tokens.first()?.as_str();
    let second = tokens.get(1).map(|s| s.as_str());

    // Two-word key check (wins over single-word).
    if let Some(second) = second {
        let two_word = format!("{first} {second}");
        if let Some(prefix) = config.mappings.get(&two_word) {
            return Some(PrefixMatch::Confirmed { key: two_word, prefix: prefix.clone() });
        }
    }

    // Single-word key check.
    if let Some(prefix) = config.mappings.get(first) {
        return Some(PrefixMatch::Confirmed { key: first.to_string(), prefix: prefix.clone() });
    }

    // Candidate fallback — use the first candidate prefix.
    if let Some(candidate) = config.candidate_prefixes.first() {
        return Some(PrefixMatch::Candidate {
            key: first.to_string(),
            prefix: candidate.clone(),
        });
    }

    None
}

/// A pending candidate probe: we applied a speculative prefix and need post-hook
/// learning to confirm or discard it.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeEntry {
    /// The command key (first word or two-word key) that was matched.
    pub key: String,
    /// The candidate prefix that was applied.
    pub prefix: Vec<String>,
    /// The original command before prefix was prepended (for matching in post-hook).
    pub original_command: String,
}

/// Result of rewriting a full command string.
#[derive(Debug, Clone)]
pub struct RewriteResult {
    /// The rewritten command (may be identical to input if nothing matched).
    pub rewritten: String,
    /// Candidate probes that need post-hook learning.
    pub probes: Vec<ProbeEntry>,
}

/// Rewrite `cmd` by prepending learned rx prefixes to each shell segment.
///
/// Segments containing `$(` or backticks are passed through unchanged.
/// Returns the rewritten command and any speculative candidate probes recorded.
pub fn rewrite_command(cmd: &str, config: &RxPrefixConfig) -> RewriteResult {
    let mut segs = split_segments(cmd);
    let mut probes = Vec::new();

    for seg in &mut segs {
        let trimmed = seg.text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(m) = lookup_prefix(trimmed, config) else {
            continue;
        };
        let (key, prefix, is_candidate) = match m {
            PrefixMatch::Confirmed { key, prefix } => (key, prefix, false),
            PrefixMatch::Candidate { key, prefix } => (key, prefix, true),
        };
        // Preserve leading and trailing whitespace from the original segment.
        let leading_len = seg.text.len() - seg.text.trim_start().len();
        let trailing_len = seg.text.len() - seg.text.trim_end().len();
        let leading = &seg.text[..leading_len];
        let trailing = &seg.text[seg.text.len() - trailing_len..];
        let prefix_str = prefix.join(" ");
        seg.text = format!("{leading}{prefix_str} {trimmed}{trailing}");

        if is_candidate {
            probes.push(ProbeEntry {
                key,
                prefix,
                original_command: cmd.to_string(),
            });
        }
    }

    RewriteResult { rewritten: rejoin(&segs), probes }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_simple_pipeline() {
        let segs = split_segments("cargo build | tail -5");
        assert_eq!(segs, vec![
            Segment { text: "cargo build ".to_string(), sep: Some("|".to_string()) },
            Segment { text: " tail -5".to_string(), sep: None },
        ]);
    }

    #[test]
    fn split_and_and() {
        let segs = split_segments("git add -A && git commit -m 'msg'");
        assert_eq!(segs, vec![
            Segment { text: "git add -A ".to_string(), sep: Some("&&".to_string()) },
            Segment { text: " git commit -m 'msg'".to_string(), sep: None },
        ]);
    }

    #[test]
    fn split_semicolon() {
        let segs = split_segments("echo a; echo b");
        assert_eq!(segs, vec![
            Segment { text: "echo a".to_string(), sep: Some(";".to_string()) },
            Segment { text: " echo b".to_string(), sep: None },
        ]);
    }

    #[test]
    fn split_or_or() {
        let segs = split_segments("cargo check || echo failed");
        assert_eq!(segs, vec![
            Segment { text: "cargo check ".to_string(), sep: Some("||".to_string()) },
            Segment { text: " echo failed".to_string(), sep: None },
        ]);
    }

    #[test]
    fn split_no_separator_is_single_segment() {
        let segs = split_segments("cargo test --workspace");
        assert_eq!(segs, vec![
            Segment { text: "cargo test --workspace".to_string(), sep: None },
        ]);
    }

    #[test]
    fn rejoin_preserves_separators() {
        let segs = vec![
            Segment { text: "cargo build ".to_string(), sep: Some("|".to_string()) },
            Segment { text: " tail -5".to_string(), sep: None },
        ];
        assert_eq!(rejoin(&segs), "cargo build | tail -5");
    }

    #[test]
    fn split_empty_string_returns_single_empty_segment() {
        let segs = split_segments("");
        assert_eq!(segs, vec![Segment { text: "".to_string(), sep: None }]);
    }

    #[test]
    fn split_separator_only_returns_two_segments() {
        let segs = split_segments("&&");
        assert_eq!(segs, vec![
            Segment { text: "".to_string(), sep: Some("&&".to_string()) },
            Segment { text: "".to_string(), sep: None },
        ]);
    }

    #[test]
    fn split_trailing_separator_produces_empty_last_segment() {
        let segs = split_segments("echo a; ");
        assert_eq!(segs, vec![
            Segment { text: "echo a".to_string(), sep: Some(";".to_string()) },
            Segment { text: " ".to_string(), sep: None },
        ]);
    }

    #[test]
    fn rejoin_and_and() {
        let segs = vec![
            Segment { text: "git add -A ".to_string(), sep: Some("&&".to_string()) },
            Segment { text: " git commit -m 'msg'".to_string(), sep: None },
        ];
        assert_eq!(rejoin(&segs), "git add -A && git commit -m 'msg'");
    }

    #[test]
    fn parse_prefixes_toml_mappings() {
        let toml = r#"
learn_on_successful_fallback = true
candidate_prefixes = [["op", "plugin", "run", "--"]]

[mappings]
gh = ["op", "plugin", "run", "--"]
cargo = ["op", "plugin", "run", "--"]
"#;
        let config: RxPrefixConfig = toml::from_str(toml).unwrap();
        assert_eq!(
            config.mappings.get("gh"),
            Some(&vec![
                "op".to_string(),
                "plugin".to_string(),
                "run".to_string(),
                "--".to_string()
            ])
        );
        assert_eq!(
            config.mappings.get("cargo"),
            Some(&vec![
                "op".to_string(),
                "plugin".to_string(),
                "run".to_string(),
                "--".to_string()
            ])
        );
        assert_eq!(
            config.candidate_prefixes,
            vec![vec![
                "op".to_string(),
                "plugin".to_string(),
                "run".to_string(),
                "--".to_string()
            ]]
        );
        assert!(config.learn_on_successful_fallback);
    }

    #[test]
    fn parse_prefixes_toml_empty() {
        let config: RxPrefixConfig = toml::from_str("").unwrap();
        assert!(config.mappings.is_empty());
        assert!(config.candidate_prefixes.is_empty());
        assert!(!config.learn_on_successful_fallback);
    }

    fn make_store(mappings: &[(&str, &[&str])], candidates: &[&[&str]]) -> FakePrefixStore {
        FakePrefixStore {
            config: RxPrefixConfig {
                mappings: mappings
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
                    .collect(),
                candidate_prefixes: candidates
                    .iter()
                    .map(|c| c.iter().map(|s| s.to_string()).collect())
                    .collect(),
                learn_on_successful_fallback: false,
            },
            written: std::cell::RefCell::new(None),
        }
    }

    #[test]
    fn lookup_single_word_key_matches() {
        let store = make_store(&[("gh", &["op", "plugin", "run", "--"])], &[]);
        let result = lookup_prefix("gh issue list", &store.load());
        assert_eq!(
            result,
            Some(PrefixMatch::Confirmed {
                key: "gh".to_string(),
                prefix: vec![
                    "op".to_string(),
                    "plugin".to_string(),
                    "run".to_string(),
                    "--".to_string()
                ],
            })
        );
    }

    #[test]
    fn lookup_two_word_key_wins_over_single() {
        let store = make_store(
            &[
                ("cargo", &["op", "plugin", "run", "--"]),
                ("cargo test", &["dotenvx", "run", "--"]),
            ],
            &[],
        );
        let result = lookup_prefix("cargo test --workspace", &store.load());
        assert_eq!(
            result,
            Some(PrefixMatch::Confirmed {
                key: "cargo test".to_string(),
                prefix: vec!["dotenvx".to_string(), "run".to_string(), "--".to_string()],
            })
        );
    }

    #[test]
    fn lookup_no_match_no_candidates_returns_none() {
        let store = make_store(&[], &[]);
        let result = lookup_prefix("echo hello", &store.load());
        assert_eq!(result, None);
    }

    #[test]
    fn lookup_no_match_with_candidate_returns_candidate() {
        let store = make_store(&[], &[&["op", "plugin", "run", "--"]]);
        let result = lookup_prefix("gh issue list", &store.load());
        assert_eq!(
            result,
            Some(PrefixMatch::Candidate {
                key: "gh".to_string(),
                prefix: vec![
                    "op".to_string(),
                    "plugin".to_string(),
                    "run".to_string(),
                    "--".to_string()
                ],
            })
        );
    }

    #[test]
    fn lookup_skips_command_with_subshell() {
        let store = make_store(&[("gh", &["op", "plugin", "run", "--"])], &[]);
        let result = lookup_prefix("$(gh issue list)", &store.load());
        assert_eq!(result, None);
    }

    #[test]
    fn lookup_skips_command_with_backtick() {
        let store = make_store(&[("gh", &["op", "plugin", "run", "--"])], &[]);
        let result = lookup_prefix("`gh issue list`", &store.load());
        assert_eq!(result, None);
    }

    #[test]
    fn rewrite_simple_command_with_confirmed_prefix() {
        let store = make_store(&[("gh", &["op", "plugin", "run", "--"])], &[]);
        let result = rewrite_command("gh issue list", &store.load());
        assert_eq!(result.rewritten, "op plugin run -- gh issue list");
        assert!(result.probes.is_empty());
    }

    #[test]
    fn rewrite_pipeline_rewrites_first_segment_only() {
        let store = make_store(&[("gh", &["op", "plugin", "run", "--"])], &[]);
        let result = rewrite_command("gh issue list | tail -5", &store.load());
        assert_eq!(result.rewritten, "op plugin run -- gh issue list | tail -5");
        assert!(result.probes.is_empty());
    }

    #[test]
    fn rewrite_compound_rewrites_each_segment_independently() {
        let store = make_store(&[("gh", &["op", "plugin", "run", "--"])], &[]);
        let result = rewrite_command("gh issue list && gh pr list", &store.load());
        assert_eq!(
            result.rewritten,
            "op plugin run -- gh issue list && op plugin run -- gh pr list"
        );
    }

    #[test]
    fn rewrite_candidate_records_probe() {
        let store = make_store(&[], &[&["op", "plugin", "run", "--"]]);
        let result = rewrite_command("gh issue list", &store.load());
        assert_eq!(result.rewritten, "op plugin run -- gh issue list");
        assert_eq!(result.probes.len(), 1);
        assert_eq!(result.probes[0].key, "gh");
        assert_eq!(result.probes[0].prefix, vec!["op", "plugin", "run", "--"]);
        assert_eq!(result.probes[0].original_command, "gh issue list");
    }

    #[test]
    fn rewrite_no_match_returns_unchanged() {
        let store = make_store(&[], &[]);
        let result = rewrite_command("echo hello", &store.load());
        assert_eq!(result.rewritten, "echo hello");
        assert!(result.probes.is_empty());
    }

    #[test]
    fn rewrite_unchanged_when_already_prefixed() {
        // "op" has no mapping, so the command passes through unchanged.
        let store = make_store(&[("gh", &["op", "plugin", "run", "--"])], &[]);
        let result = rewrite_command("op plugin run -- gh issue list", &store.load());
        assert_eq!(result.rewritten, "op plugin run -- gh issue list");
    }

    #[test]
    fn fake_store_load_returns_injected_config() {
        let config = RxPrefixConfig {
            mappings: std::collections::HashMap::from([(
                "gh".to_string(),
                vec![
                    "op".to_string(),
                    "plugin".to_string(),
                    "run".to_string(),
                    "--".to_string(),
                ],
            )]),
            candidate_prefixes: vec![],
            learn_on_successful_fallback: true,
        };
        let store = FakePrefixStore {
            config: config.clone(),
            written: std::cell::RefCell::new(None),
        };
        let loaded = store.load();
        assert_eq!(loaded.mappings.get("gh"), config.mappings.get("gh"));
    }
}
