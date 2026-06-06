use regex::Regex;
use serde::Deserialize;
use shell_words;
use std::fs;

use crate::config::rules_path;

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub pattern: String,
    #[serde(default)]
    pub pattern_flags: String,
    #[serde(default)]
    pub exceptions: Vec<String>,
    /// Command names this rule targets (e.g. `["grep", "rg"]`).
    /// When non-empty, the rule only fires if argv[0] of at least one pipe
    /// stage matches. When empty, falls back to raw regex matching.
    #[serde(default)]
    pub target_commands: Vec<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FailureLearning {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_block_threshold")]
    pub block_threshold: usize,
    #[serde(default = "default_window")]
    pub window_seconds: u64,
    pub state_file: Option<String>,
    #[serde(default = "default_max_entries")]
    pub max_tracked_commands: usize,
    #[serde(default = "default_cleanup")]
    pub cleanup_after_seconds: u64,
    pub message_template: Option<String>,
}

impl Default for FailureLearning {
    fn default() -> Self {
        Self {
            enabled: true,
            block_threshold: default_block_threshold(),
            window_seconds: default_window(),
            state_file: None,
            max_tracked_commands: default_max_entries(),
            cleanup_after_seconds: default_cleanup(),
            message_template: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RulesConfig {
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub failure_learning: FailureLearning,
}

fn default_true() -> bool {
    true
}
fn default_block_threshold() -> usize {
    3
}
fn default_window() -> u64 {
    300
}
fn default_max_entries() -> usize {
    200
}
fn default_cleanup() -> u64 {
    3600
}

pub fn load() -> RulesConfig {
    let path = rules_path();
    let Ok(content) = fs::read_to_string(&path) else {
        return RulesConfig {
            rules: vec![],
            failure_learning: FailureLearning::default(),
        };
    };
    serde_json::from_str(&content).unwrap_or(RulesConfig {
        rules: vec![],
        failure_learning: FailureLearning::default(),
    })
}

/// Returns true if `target_commands` is non-empty and at least one pipe stage's
/// argv[0] matches a target command name.  Returns true when `target_commands`
/// is empty (legacy fallback — raw regex applies to the whole string).
fn targets_match(command: &str, targets: &[String]) -> bool {
    if targets.is_empty() {
        return true; // no gate — legacy behaviour
    }
    let cmds = crate::pipeline::pipe_stage_commands(command);
    cmds.iter().any(|c| targets.iter().any(|t| t == c))
}

/// Identify the pipe stage whose argv[0] triggered the rule, for use in deny
/// messages.  Returns `None` if no specific stage matched (legacy rules).
fn triggering_stage<'a>(command: &'a str, targets: &[String]) -> Option<&'a str> {
    if targets.is_empty() {
        return None;
    }
    let stages = crate::pipeline::pipe_stages(command);
    for stage in stages {
        let argv = shell_words::split(stage.trim()).unwrap_or_default();
        if let Some(name) = argv.first()
            && targets.iter().any(|t| t == name)
        {
            return Some(stage);
        }
    }
    None
}

/// Build the regex for a rule, applying case-insensitive flag when needed.
fn build_regex(rule: &Rule) -> Option<Regex> {
    let pattern_str = if rule.pattern_flags.contains('i') || rule.pattern.contains("(?i)") {
        format!("(?i){}", rule.pattern)
    } else {
        rule.pattern.clone()
    };
    Regex::new(&pattern_str).ok()
}

/// Returns the id of the first matching rule (respecting exceptions), None otherwise.
/// Used by discover to attribute commands to the rule that would actually fire.
pub fn matched_rule_id(command: &str, rules: &[Rule]) -> Option<String> {
    for rule in rules {
        if !rule.enabled {
            continue;
        }
        if !targets_match(command, &rule.target_commands) {
            continue;
        }
        let Some(re) = build_regex(rule) else {
            continue;
        };
        if !re.is_match(command) {
            continue;
        }
        let excepted = rule.exceptions.iter().any(|exc| {
            Regex::new(exc)
                .map(|re| re.is_match(command))
                .unwrap_or(false)
        });
        if excepted {
            continue;
        }
        return Some(rule.id.clone());
    }
    None
}

/// Returns `(rule_id, deny_message)` if any rule matches, None otherwise.
///
/// When a rule has `target_commands`, the deny message is enriched with the
/// specific pipe stage that triggered the block.
pub fn check(command: &str, rules: &[Rule]) -> Option<(String, String)> {
    for rule in rules {
        if !rule.enabled {
            continue;
        }

        // Gate: if the rule declares target commands, skip unless argv[0] of
        // at least one pipe stage matches.  Regex + exceptions still run
        // against the original full segment to preserve exception semantics
        // (e.g. `\|\s*grep`).
        if !targets_match(command, &rule.target_commands) {
            continue;
        }

        let Some(re) = build_regex(rule) else {
            continue;
        };
        if !re.is_match(command) {
            continue;
        }

        // Check exceptions — allow if any exception matches
        let excepted = rule.exceptions.iter().any(|exc| {
            Regex::new(exc)
                .map(|re| re.is_match(command))
                .unwrap_or(false)
        });
        if excepted {
            continue;
        }

        let base_msg = rule
            .message
            .clone()
            .unwrap_or_else(|| format!("Blocked by rule '{}'.", rule.id));

        // Enrich message with the triggering stage when available
        let msg = if let Some(stage) = triggering_stage(command, &rule.target_commands) {
            format!("{base_msg}\n\nBlocked command: `{stage}`")
        } else {
            base_msg
        };

        return Some((rule.id.clone(), msg));
    }
    None
}

/// Pipeline-aware variant of `check`. Splits `command` on sequential operators
/// (`&&`, `||`, `;`) and returns a block decision if any segment matches a rule.
pub fn check_pipeline(command: &str, rules: &[Rule]) -> Option<(String, String)> {
    crate::pipeline::sequential_segments(command)
        .into_iter()
        .find_map(|seg| check(seg, rules))
}

/// Pipeline-aware variant of `matched_rule_id`.
pub fn matched_rule_id_pipeline(command: &str, rules: &[Rule]) -> Option<String> {
    crate::pipeline::sequential_segments(command)
        .into_iter()
        .find_map(|seg| matched_rule_id(seg, rules))
}

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Proof: FailureLearning::default() produces valid config.
    #[kani::proof]
    #[kani::unwind(1)]
    fn failure_learning_defaults_valid() {
        let fl = FailureLearning::default();
        assert!(fl.enabled);
        assert!(fl.block_threshold > 0, "threshold must be positive");
        assert!(fl.window_seconds > 0, "window must be positive");
        assert!(fl.max_tracked_commands > 0, "max_tracked must be positive");
        assert!(
            fl.cleanup_after_seconds >= fl.window_seconds,
            "cleanup must be >= window"
        );
    }

    /// Proof: default_block_threshold is always >= 1.
    #[kani::proof]
    #[kani::unwind(1)]
    fn block_threshold_at_least_one() {
        let t = default_block_threshold();
        assert!(t >= 1, "block_threshold must be at least 1");
    }

    /// Proof: default_window is always > 0.
    #[kani::proof]
    #[kani::unwind(1)]
    fn window_positive() {
        let w = default_window();
        assert!(w > 0, "window must be positive");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rule(id: &str, pattern: &str) -> Rule {
        Rule {
            id: id.to_string(),
            enabled: true,
            pattern: pattern.to_string(),
            pattern_flags: String::new(),
            exceptions: vec![],
            target_commands: vec![],
            message: None,
        }
    }

    fn make_targeted_rule(id: &str, pattern: &str, targets: Vec<&str>) -> Rule {
        Rule {
            id: id.to_string(),
            enabled: true,
            pattern: pattern.to_string(),
            pattern_flags: String::new(),
            exceptions: vec![],
            target_commands: targets.into_iter().map(String::from).collect(),
            message: Some(format!("Use the dedicated tool instead of {id}.")),
        }
    }

    #[test]
    fn rule_matches_pattern() {
        let rules = vec![make_rule("no-grep", r"\bgrep\b")];
        let result = check("grep foo .", &rules);
        assert!(result.is_some());
        assert!(result.unwrap().0.contains("no-grep"));
    }

    #[test]
    fn rule_no_match() {
        let rules = vec![make_rule("no-grep", r"\bgrep\b")];
        assert!(check("ls -la", &rules).is_none());
    }

    #[test]
    fn rule_case_insensitive_flag() {
        let mut rule = make_rule("no-grep", r"\bgrep\b");
        rule.pattern_flags = "i".to_string();
        assert!(check("GREP foo .", &[rule]).is_some());
    }

    #[test]
    fn rule_exception_bypasses_block() {
        let mut rule = make_rule("no-grep", r"\bgrep\b");
        rule.exceptions = vec![r"\| grep".to_string()];
        assert!(check("cmd | grep foo", &[rule]).is_none());
    }

    #[test]
    fn rule_disabled_skipped() {
        let mut rule = make_rule("no-grep", r"\bgrep\b");
        rule.enabled = false;
        assert!(check("grep foo .", &[rule]).is_none());
    }

    #[test]
    fn rule_bad_regex_skipped() {
        let rule = make_rule("bad", r"[invalid");
        assert!(check("anything", &[rule]).is_none());
    }

    #[test]
    fn no_rules_allows_all() {
        assert!(check("grep foo .", &[]).is_none());
    }

    // ── check_pipeline ────────────────────────────────────────────────────

    #[test]
    fn pipeline_clean_command_passes() {
        let rules = vec![make_rule("no-grep", r"\bgrep\b")];
        assert!(check_pipeline("cargo build && git status", &rules).is_none());
    }

    #[test]
    fn pipeline_second_segment_blocked() {
        let rules = vec![make_rule("no-grep", r"\bgrep\b")];
        let result = check_pipeline("cargo build && grep foo .", &rules);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "no-grep");
    }

    #[test]
    fn pipeline_first_segment_blocked() {
        let rules = vec![make_rule("no-grep", r"\bgrep\b")];
        assert!(check_pipeline("grep foo . && cargo test", &rules).is_some());
    }

    #[test]
    fn pipeline_pipe_exception_still_works() {
        // `| grep` exception matches the whole piped segment — pipe is NOT split
        let mut rule = make_rule("no-grep", r"\bgrep\b");
        rule.exceptions = vec![r"\| grep".to_string()];
        assert!(check_pipeline("cmd | grep foo", &[rule]).is_none());
    }

    #[test]
    fn pipeline_semicolon_split() {
        let rules = vec![make_rule("no-grep", r"\bgrep\b")];
        assert!(check_pipeline("echo hi; grep foo .", &rules).is_some());
    }

    #[test]
    fn pipeline_or_or_split() {
        let rules = vec![make_rule("no-grep", r"\bgrep\b")];
        assert!(check_pipeline("cargo build || grep foo .", &rules).is_some());
    }

    #[test]
    fn rule_custom_message_returned() {
        let mut rule = make_rule("no-grep", r"\bgrep\b");
        rule.message = Some("Use the Grep tool.".to_string());
        assert_eq!(
            check("grep foo .", &[rule]).unwrap().1,
            "Use the Grep tool."
        );
    }

    // ── no-nvm-use-mise rule ──────────────────────────────────────────────

    fn nvm_rule() -> Rule {
        Rule {
            id: "no-nvm-use-mise".to_string(),
            enabled: true,
            pattern: r"(?:^|\s)nvm\b".to_string(),
            pattern_flags: String::new(),
            exceptions: vec![],
            target_commands: vec!["nvm".to_string()],
            message: Some(
                "Use `mise use node@<version>` instead of nvm. \
                 Example: `mise use node@20` or `mise use --global node@lts`."
                    .to_string(),
            ),
        }
    }

    #[test]
    fn nvm_rule_blocks_nvm_install() {
        assert!(check("nvm install 20", &[nvm_rule()]).is_some());
    }

    #[test]
    fn nvm_rule_blocks_nvm_use() {
        assert!(check("nvm use 18", &[nvm_rule()]).is_some());
    }

    #[test]
    fn nvm_rule_blocks_nvm_alias() {
        assert!(check("nvm alias default 20", &[nvm_rule()]).is_some());
    }

    #[test]
    fn nvm_rule_blocks_nvm_ls() {
        assert!(check("nvm ls", &[nvm_rule()]).is_some());
    }

    #[test]
    fn nvm_rule_passes_mise_use_node() {
        assert!(check("mise use node@20", &[nvm_rule()]).is_none());
    }

    #[test]
    fn nvm_rule_message_mentions_mise() {
        let (_, msg) = check("nvm install 20", &[nvm_rule()]).unwrap();
        assert!(msg.contains("mise"));
    }

    #[test]
    fn nvm_rule_id_is_correct() {
        let (rule_id, _) = check("nvm install 20", &[nvm_rule()]).unwrap();
        assert_eq!(rule_id, "no-nvm-use-mise");
    }

    // ── target_commands gating ────────────────────────────────────────────

    fn grep_targeted_rule() -> Rule {
        let mut r = make_targeted_rule("no-grep", r"\bgrep\b", vec!["grep", "rg"]);
        r.exceptions = vec![r"\|\s*grep".to_string()];
        r
    }

    fn find_targeted_rule() -> Rule {
        make_targeted_rule("no-find", r#"\bfind\s+[./~$"']"#, vec!["find"])
    }

    fn cat_targeted_rule() -> Rule {
        make_targeted_rule("no-cat", r"\bcat\s+[^|<]", vec!["cat"])
    }

    // ── true positives: targeted rules still block actual commands ────────

    #[test]
    fn targeted_grep_blocks_direct_invocation() {
        let rules = vec![grep_targeted_rule()];
        assert!(check("grep foo .", &rules).is_some());
    }

    #[test]
    fn targeted_grep_blocks_in_pipeline_stage() {
        // `grep` is argv[0] of the second pipe stage
        let rules = vec![grep_targeted_rule()];
        // Note: exception `\|\s*grep` will match here, so this is excepted.
        // But a standalone `grep` still blocks:
        assert!(check("grep -r pattern src/", &rules).is_some());
    }

    #[test]
    fn targeted_find_blocks_direct_invocation() {
        let rules = vec![find_targeted_rule()];
        assert!(check("find . -name '*.rs'", &rules).is_some());
    }

    #[test]
    fn targeted_cat_blocks_direct_invocation() {
        let rules = vec![cat_targeted_rule()];
        assert!(check("cat somefile.txt", &rules).is_some());
    }

    // ── false positives: these must NOT be blocked ───────────────────────

    #[test]
    fn targeted_grep_allows_word_in_commit_message() {
        let rules = vec![grep_targeted_rule()];
        assert!(check(r#"git commit -m "grep patterns are tricky""#, &rules).is_none());
    }

    #[test]
    fn targeted_grep_allows_word_in_echo() {
        let rules = vec![grep_targeted_rule()];
        assert!(check(r#"echo "use grep for searching""#, &rules).is_none());
    }

    #[test]
    fn targeted_grep_allows_word_in_test_name() {
        let rules = vec![grep_targeted_rule()];
        assert!(check("cargo test grep_test", &rules).is_none());
    }

    #[test]
    fn targeted_find_allows_word_in_commit_message() {
        let rules = vec![find_targeted_rule()];
        assert!(check(r#"git commit -m "find .ctx stuff""#, &rules).is_none());
    }

    #[test]
    fn targeted_find_allows_word_in_echo() {
        let rules = vec![find_targeted_rule()];
        assert!(check(r#"echo "could not find .config""#, &rules).is_none());
    }

    #[test]
    fn targeted_cat_allows_word_in_echo() {
        let rules = vec![cat_targeted_rule()];
        assert!(check(r#"echo "use cat /dev/null trick""#, &rules).is_none());
    }

    // ── exception compatibility with targeted rules ──────────────────────

    #[test]
    fn targeted_grep_pipe_exception_still_works() {
        // `| grep` exception applies against the full segment
        let rules = vec![grep_targeted_rule()];
        assert!(check("cargo test | grep passed", &rules).is_none());
    }

    // ── deny message includes triggering stage ───────────────────────────

    #[test]
    fn targeted_deny_message_includes_blocked_stage() {
        let rules = vec![grep_targeted_rule()];
        let (_, msg) = check("grep foo .", &rules).unwrap();
        assert!(msg.contains("Blocked command: `grep foo .`"), "msg: {msg}");
    }

    // ── legacy rules (no target_commands) still work ─────────────────────

    #[test]
    fn legacy_rule_without_targets_still_matches() {
        let rules = vec![make_rule("no-grep", r"\bgrep\b")];
        // Legacy behaviour: matches the word anywhere in the string
        assert!(check("echo grep is useful", &rules).is_some());
    }
}
