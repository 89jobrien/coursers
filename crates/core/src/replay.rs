//! Replay a list of commands through the current ruleset.
//!
//! No side effects: does not write to stats, state, or any file.
//! Produces a `ReplayReport` showing which commands would be blocked,
//! which would pass, and which rule would fire.

use crate::rules::{Rule, check};

#[derive(Debug, Clone, PartialEq)]
pub enum ReplayVerdict {
    Blocked { rule_id: String, message: String },
    Pass,
}

#[derive(Debug, Clone)]
pub struct ReplayEntry {
    pub command: String,
    pub verdict: ReplayVerdict,
}

#[derive(Debug, Default)]
pub struct ReplayReport {
    pub entries: Vec<ReplayEntry>,
    pub blocked: usize,
    pub passed: usize,
}

/// Run `commands` through `rules` and return a report with per-command verdicts.
/// Pure function — no I/O.
pub fn replay(commands: &[String], rules: &[Rule]) -> ReplayReport {
    let mut report = ReplayReport::default();

    for cmd in commands {
        let verdict = match check(cmd, rules) {
            Some((rule_id, message)) => ReplayVerdict::Blocked { rule_id, message },
            None => ReplayVerdict::Pass,
        };

        if matches!(verdict, ReplayVerdict::Blocked { .. }) {
            report.blocked += 1;
        } else {
            report.passed += 1;
        }

        report.entries.push(ReplayEntry {
            command: cmd.clone(),
            verdict,
        });
    }

    report
}

/// Format a `ReplayReport` as human-readable text.
pub fn format_text(report: &ReplayReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Replay: {} commands — {} blocked, {} passed\n\n",
        report.entries.len(),
        report.blocked,
        report.passed
    ));

    for entry in &report.entries {
        match &entry.verdict {
            ReplayVerdict::Blocked { rule_id, .. } => {
                out.push_str(&format!("  BLOCK [{}]  {}\n", rule_id, entry.command));
            }
            ReplayVerdict::Pass => {
                out.push_str(&format!("  pass        {}\n", entry.command));
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::Rule;

    fn rule(id: &str, pattern: &str) -> Rule {
        Rule {
            id: id.to_string(),
            enabled: true,
            pattern: pattern.to_string(),
            pattern_flags: String::new(),
            exceptions: vec![],
            target_commands: vec![],
            message: Some(format!("Use tool instead of {id}.")),
        }
    }

    #[test]
    fn replay_empty_commands_returns_empty_report() {
        let report = replay(&[], &[]);
        assert_eq!(report.entries.len(), 0);
        assert_eq!(report.blocked, 0);
        assert_eq!(report.passed, 0);
    }

    #[test]
    fn replay_all_pass_when_no_rules() {
        let cmds = vec!["grep foo .".to_string(), "ls -la".to_string()];
        let report = replay(&cmds, &[]);
        assert_eq!(report.passed, 2);
        assert_eq!(report.blocked, 0);
    }

    #[test]
    fn replay_blocks_matching_command() {
        let cmds = vec!["grep foo .".to_string()];
        let rules = vec![rule("no-grep", r"\bgrep\b")];
        let report = replay(&cmds, &rules);
        assert_eq!(report.blocked, 1);
        assert_eq!(report.passed, 0);
        assert!(matches!(
            &report.entries[0].verdict,
            ReplayVerdict::Blocked { rule_id, .. } if rule_id == "no-grep"
        ));
    }

    #[test]
    fn replay_passes_non_matching_command() {
        let cmds = vec!["cargo build".to_string()];
        let rules = vec![rule("no-grep", r"\bgrep\b")];
        let report = replay(&cmds, &rules);
        assert_eq!(report.passed, 1);
        assert_eq!(report.blocked, 0);
    }

    #[test]
    fn replay_mixed_commands_counted_correctly() {
        let cmds = vec![
            "grep foo .".to_string(),
            "cargo build".to_string(),
            "grep bar baz".to_string(),
        ];
        let rules = vec![rule("no-grep", r"\bgrep\b")];
        let report = replay(&cmds, &rules);
        assert_eq!(report.blocked, 2);
        assert_eq!(report.passed, 1);
    }

    #[test]
    fn format_text_contains_summary_line() {
        let cmds = vec!["grep foo .".to_string()];
        let rules = vec![rule("no-grep", r"\bgrep\b")];
        let report = replay(&cmds, &rules);
        let text = format_text(&report);
        assert!(text.contains("1 blocked"));
        assert!(text.contains("0 passed"));
    }

    #[test]
    fn format_text_shows_rule_id_for_blocked() {
        let cmds = vec!["grep foo .".to_string()];
        let rules = vec![rule("no-grep", r"\bgrep\b")];
        let report = replay(&cmds, &rules);
        let text = format_text(&report);
        assert!(text.contains("no-grep"));
    }
}
