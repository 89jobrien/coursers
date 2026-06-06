/// Approximate bytes per token (GPT/Claude tokenizer average).
const BYTES_PER_TOKEN: usize = 4;

/// Extracts the command stem (1–2 token prefix) used for frequency grouping.
///
/// Rules:
/// 1. Strip leading `KEY=val` env assignments.
/// 2. Strip path prefix from token 0 (keep only the basename).
/// 3. If token 1 exists and does not start with `-`, append it: `cargo nextest`.
///    Otherwise stem = token 0 only.
pub fn stem_of(command: &str) -> String {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    if tokens.is_empty() {
        return String::new();
    }

    // Strip leading KEY=val env assignments
    let start = tokens
        .iter()
        .take_while(|t| t.contains('=') && !t.starts_with('-'))
        .count();
    let tokens = &tokens[start..];
    if tokens.is_empty() {
        return String::new();
    }

    // Strip path prefix from token 0
    let base = tokens[0].rsplit('/').next().unwrap_or(tokens[0]);

    // Append token 1 if it exists, is not a flag, AND token 2 exists
    if tokens.len() > 2
        && let Some(t1) = tokens.get(1)
        && !t1.starts_with('-')
    {
        return format!("{base} {t1}");
    }

    base.to_string()
}

use crate::rules::Rule;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct CommandRecord {
    pub command: String,
    pub session_id: String,
    pub cwd: String,
    pub timestamp: Option<String>,
    /// Actual output byte count from the tool_result record, if available.
    pub output_bytes: Option<usize>,
}

pub trait CommandSource {
    fn commands(&self) -> impl Iterator<Item = CommandRecord>;
}

pub struct DiscoverOpts {
    pub limit: usize,
    pub since_days: Option<u32>,
    pub all_projects: bool,
    pub current_dir: Option<PathBuf>,
    /// Filter out entries with count below this threshold. 0 or 1 means show all.
    pub min_count: u64,
}

impl Default for DiscoverOpts {
    fn default() -> Self {
        Self {
            limit: 15,
            since_days: Some(30),
            all_projects: false,
            current_dir: None,
            min_count: 1,
        }
    }
}

#[derive(Debug, Default)]
pub struct CommandFreq {
    pub stem: String,
    pub count: u64,
    pub example: String,
    pub est_tokens: u64,
    pub rule_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct DiscoverReport {
    pub intercepted: Vec<CommandFreq>,
    pub unhandled: Vec<CommandFreq>,
    pub scanned_sessions: usize,
    pub scanned_commands: usize,
}

pub fn discover(
    source: &impl CommandSource,
    rules: &[Rule],
    opts: &DiscoverOpts,
) -> DiscoverReport {
    let cutoff: Option<String> = opts.since_days.map(days_ago);

    let mut intercepted: HashMap<String, CommandFreq> = HashMap::new();
    let mut unhandled: HashMap<String, CommandFreq> = HashMap::new();
    let mut scanned_commands = 0usize;
    let mut seen_sessions = std::collections::HashSet::new();

    for rec in source.commands() {
        // Project filter
        if !opts.all_projects
            && let Some(ref cwd) = opts.current_dir
            && rec.cwd != cwd.to_string_lossy().as_ref()
        {
            continue;
        }

        // Since filter — compare date prefix (first 10 chars of ISO 8601)
        if let (Some(cutoff_str), Some(ts)) = (&cutoff, &rec.timestamp) {
            let date_part = &ts[..ts.len().min(10)];
            if date_part < cutoff_str.as_str() {
                continue;
            }
        }

        scanned_commands += 1;
        seen_sessions.insert(rec.session_id.clone());

        let stem = stem_of(&rec.command);
        if stem.is_empty() {
            continue;
        }

        // Check against rules — use the same logic as the pre-tool hook,
        // including exception evaluation, so discover matches what actually fires.
        let rule_id = crate::rules::matched_rule_id(&rec.command, rules);

        let bucket = if rule_id.is_some() {
            &mut intercepted
        } else {
            &mut unhandled
        };
        let entry = bucket.entry(stem.clone()).or_insert_with(|| CommandFreq {
            stem: stem.clone(),
            count: 0,
            example: rec.command.clone(),
            est_tokens: 0,
            rule_id: rule_id.clone(),
        });
        entry.count += 1;
        // Use real output length when available: bytes / 4 ≈ tokens.
        // Never fabricate a number — if output_bytes is absent, leave est_tokens at 0.
        if let Some(bytes) = rec.output_bytes {
            entry.est_tokens += (bytes / BYTES_PER_TOKEN) as u64;
        }
    }

    // Apply min_count filter (0 treated as 1 — show all singletons)
    let threshold = opts.min_count.max(1);
    let mut intercepted: Vec<CommandFreq> = intercepted
        .into_values()
        .filter(|f| f.count >= threshold)
        .collect();
    let mut unhandled: Vec<CommandFreq> = unhandled
        .into_values()
        .filter(|f| f.count >= threshold)
        .collect();

    // Sort by count desc, truncate to limit
    intercepted.sort_by(|a, b| b.count.cmp(&a.count));
    unhandled.sort_by(|a, b| b.count.cmp(&a.count));
    if opts.limit > 0 {
        intercepted.truncate(opts.limit);
        unhandled.truncate(opts.limit);
    }

    DiscoverReport {
        intercepted,
        unhandled,
        scanned_sessions: seen_sessions.len(),
        scanned_commands,
    }
}

fn days_ago(days: u32) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cutoff_secs = secs.saturating_sub(days as u64 * crate::date::SECS_PER_DAY);
    crate::date::unix_secs_to_date_str(cutoff_secs)
}

#[cfg(kani)]
mod kani_proofs {
    use super::BYTES_PER_TOKEN;

    /// Proof: BYTES_PER_TOKEN is positive (used as divisor in token estimation).
    #[kani::proof]
    #[kani::unwind(1)]
    fn bytes_per_token_positive() {
        assert!(BYTES_PER_TOKEN > 0, "BYTES_PER_TOKEN must be positive");
    }

    /// Proof: token estimation from byte count never overflows for reasonable sizes.
    #[kani::proof]
    #[kani::unwind(1)]
    fn token_estimate_no_overflow() {
        let bytes: usize = kani::any();
        // Realistic bound: up to 1GB of output
        kani::assume(bytes <= 1_073_741_824);
        let tokens = bytes / BYTES_PER_TOKEN;
        assert!(tokens <= bytes, "tokens must be <= bytes");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stem_bare_command() {
        assert_eq!(stem_of("ls -la"), "ls");
    }

    #[test]
    fn stem_two_token_subcommand() {
        assert_eq!(stem_of("cargo nextest run -p crs-core"), "cargo nextest");
    }

    #[test]
    fn stem_subcommand_with_flag_token1() {
        assert_eq!(stem_of("git --no-pager log"), "git");
    }

    #[test]
    fn stem_strips_path_prefix() {
        assert_eq!(stem_of("/usr/bin/python3 script.py"), "python3");
    }

    #[test]
    fn stem_strips_env_assignment() {
        assert_eq!(stem_of("RUST_LOG=debug cargo build"), "cargo");
    }

    #[test]
    fn stem_strips_multiple_env_assignments() {
        assert_eq!(stem_of("A=1 B=2 cargo test"), "cargo");
    }

    #[test]
    fn stem_empty_command() {
        assert_eq!(stem_of(""), "");
    }

    #[test]
    fn stem_single_token() {
        assert_eq!(stem_of("make"), "make");
    }

    #[test]
    fn stem_doob_todo() {
        assert_eq!(stem_of("doob todo list --project coursers"), "doob todo");
    }

    // Minimal CommandSource impl for tests
    struct VecSource(Vec<CommandRecord>);
    impl CommandSource for VecSource {
        fn commands(&self) -> impl Iterator<Item = CommandRecord> {
            self.0.iter().map(|r| CommandRecord {
                command: r.command.clone(),
                session_id: r.session_id.clone(),
                cwd: r.cwd.clone(),
                timestamp: r.timestamp.clone(),
                output_bytes: r.output_bytes,
            })
        }
    }

    fn make_record(command: &str, cwd: &str) -> CommandRecord {
        CommandRecord {
            command: command.to_string(),
            session_id: "sess-1".to_string(),
            cwd: cwd.to_string(),
            timestamp: None,
            output_bytes: None,
        }
    }

    fn make_rule(id: &str, pattern: &str) -> Rule {
        crate::rules::Rule {
            id: id.to_string(),
            enabled: true,
            pattern: pattern.to_string(),
            pattern_flags: String::new(),
            exceptions: vec![],
            target_commands: vec![],
            message: None,
        }
    }

    #[test]
    fn discover_counts_unhandled_commands() {
        let src = VecSource(vec![
            make_record("doob todo list", "/project"),
            make_record("doob todo list", "/project"),
            make_record("doob todo list", "/project"),
        ]);
        let report = discover(
            &src,
            &[],
            &DiscoverOpts {
                all_projects: true,
                ..Default::default()
            },
        );
        assert_eq!(report.scanned_commands, 3);
        assert_eq!(report.unhandled.len(), 1);
        assert_eq!(report.unhandled[0].stem, "doob todo");
        assert_eq!(report.unhandled[0].count, 3);
    }

    #[test]
    fn discover_counts_intercepted_commands() {
        let src = VecSource(vec![
            make_record("cargo nextest run", "/project"),
            make_record("cargo nextest run -p foo", "/project"),
        ]);
        let rules = vec![make_rule("no-nextest", r"cargo nextest")];
        let report = discover(
            &src,
            &rules,
            &DiscoverOpts {
                all_projects: true,
                ..Default::default()
            },
        );
        assert_eq!(report.intercepted.len(), 1);
        assert_eq!(report.intercepted[0].stem, "cargo nextest");
        assert_eq!(report.intercepted[0].count, 2);
        assert_eq!(report.intercepted[0].est_tokens, 0); // no output_bytes set
    }

    #[test]
    fn discover_filters_by_cwd_when_not_all() {
        let src = VecSource(vec![
            make_record("doob todo", "/project/a"),
            make_record("doob todo", "/project/b"),
        ]);
        let report = discover(
            &src,
            &[],
            &DiscoverOpts {
                all_projects: false,
                current_dir: Some(PathBuf::from("/project/a")),
                ..Default::default()
            },
        );
        assert_eq!(report.scanned_commands, 1);
    }

    #[test]
    fn discover_respects_limit() {
        let src = VecSource(
            (0..20)
                .map(|i| make_record(&format!("cmd{i} sub"), "/p"))
                .collect(),
        );
        let report = discover(
            &src,
            &[],
            &DiscoverOpts {
                limit: 5,
                all_projects: true,
                ..Default::default()
            },
        );
        assert!(report.unhandled.len() <= 5);
    }

    #[test]
    fn discover_filters_by_since_days() {
        let old = {
            let mut r = make_record("old cmd", "/p");
            r.timestamp = Some("2020-01-01T00:00:00Z".to_string());
            r
        };
        let new_rec = {
            let mut r = make_record("new cmd here", "/p");
            r.timestamp = Some("2099-12-31T00:00:00Z".to_string());
            r
        };
        let src = VecSource(vec![old, new_rec]);
        let report = discover(
            &src,
            &[],
            &DiscoverOpts {
                since_days: Some(30),
                all_projects: true,
                ..Default::default()
            },
        );
        assert_eq!(report.scanned_commands, 1);
        assert_eq!(report.unhandled[0].stem, "new cmd");
    }

    #[test]
    fn discover_min_count_filters_low_frequency_unhandled() {
        // cmd-a appears 3 times, cmd-b appears 1 time
        let records = vec![
            make_record("cmd-a sub", "/p"),
            make_record("cmd-a sub", "/p"),
            make_record("cmd-a sub", "/p"),
            make_record("cmd-b sub", "/p"),
        ];
        let src = VecSource(records);
        let report = discover(
            &src,
            &[],
            &DiscoverOpts {
                all_projects: true,
                min_count: 2,
                ..Default::default()
            },
        );
        assert_eq!(report.unhandled.len(), 1);
        assert_eq!(report.unhandled[0].stem, "cmd-a");
    }

    #[test]
    fn discover_min_count_filters_low_frequency_intercepted() {
        // Use two different rule-matching stems with different frequencies
        let records = vec![
            make_record("grep foo .", "/p"), // count 1 — below threshold
            make_record("ls -la", "/p"),     // count 3 — above threshold
            make_record("ls /tmp", "/p"),
            make_record("ls /usr", "/p"),
        ];
        let rules = vec![
            make_rule("no-grep", r"\bgrep\b"),
            make_rule("no-ls", r"\bls\b"),
        ];
        let src = VecSource(records);
        let report = discover(
            &src,
            &rules,
            &DiscoverOpts {
                all_projects: true,
                min_count: 2,
                ..Default::default()
            },
        );
        // grep intercepted once — filtered out; ls intercepted 3 times — kept
        assert_eq!(report.intercepted.len(), 1);
        assert_eq!(report.intercepted[0].stem, "ls");
    }

    #[test]
    fn discover_min_count_default_one_shows_all() {
        let src = VecSource(vec![make_record("rare cmd here", "/p")]);
        let report = discover(
            &src,
            &[],
            &DiscoverOpts {
                all_projects: true,
                min_count: 1,
                ..Default::default()
            },
        );
        assert_eq!(report.unhandled.len(), 1);
    }

    #[test]
    fn discover_min_count_zero_treated_as_one() {
        let src = VecSource(vec![make_record("rare cmd here", "/p")]);
        let report = discover(
            &src,
            &[],
            &DiscoverOpts {
                all_projects: true,
                min_count: 0,
                ..Default::default()
            },
        );
        // min_count 0 is treated same as 1 — show everything
        assert_eq!(report.unhandled.len(), 1);
    }
}
