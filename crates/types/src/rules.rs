//! Domain configuration for blocking rules and failure learning.

use serde::Deserialize;

/// A rule that blocks a shell command matching a pattern.
// TODO(canonical-domain-contracts): migrate the remaining duplicate contracts and ports into
// coursers-types, then remove deprecated core compatibility shims after one minor release (#63).
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
    #[serde(default)]
    pub target_commands: Vec<String>,
    pub message: Option<String>,
    /// Pattern matched against running godmode task titles.
    ///
    /// A trailing `*` performs substring matching on the preceding text; without it, matching is
    /// exact.
    #[serde(default)]
    pub task_override: Option<String>,
}

/// Configuration for the failure-learning subsystem.
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

/// Root configuration loaded from the course-correct-rules JSON file.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RulesConfig {
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub failure_learning: FailureLearning,
}

/// Returns the default enabled value for rules and failure learning.
pub fn default_true() -> bool {
    true
}
/// Returns the default failure threshold of three attempts.
pub fn default_block_threshold() -> usize {
    3
}
/// Returns the default failure window of 300 seconds.
pub fn default_window() -> u64 {
    300
}
/// Returns the default limit of 200 tracked commands.
pub fn default_max_entries() -> usize {
    200
}
/// Returns the default stale-entry lifetime of 3600 seconds.
pub fn default_cleanup() -> u64 {
    3600
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_without_task_override_remains_backward_compatible() {
        let rule: Rule = serde_json::from_str(
            r#"{
                "id": "no-grep",
                "pattern": "grep",
                "message": "use Grep"
            }"#,
        )
        .unwrap();

        assert!(rule.task_override.is_none());
    }

    #[test]
    fn rule_deserializes_with_defaults() {
        let json = r#"{"id": "no-find", "pattern": "\\bfind\\b"}"#;
        let rule: Rule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.id, "no-find");
        assert!(rule.enabled);
        assert_eq!(rule.pattern_flags, "");
        assert!(rule.exceptions.is_empty());
        assert!(rule.target_commands.is_empty());
        assert!(rule.message.is_none());
    }

    #[test]
    fn rule_deserializes_with_explicit_fields() {
        let json = r#"{
            "id": "no-find",
            "enabled": false,
            "pattern": "\\bfind\\b",
            "pattern_flags": "i",
            "exceptions": ["git log"],
            "target_commands": ["find"],
            "message": "use glob instead"
        }"#;
        let rule: Rule = serde_json::from_str(json).unwrap();
        assert!(!rule.enabled);
        assert_eq!(rule.pattern_flags, "i");
        assert_eq!(rule.exceptions, vec!["git log".to_string()]);
        assert_eq!(rule.message.as_deref(), Some("use glob instead"));
    }

    #[test]
    fn failure_learning_default_matches_default_impl() {
        let json = "{}";
        let fl: FailureLearning = serde_json::from_str(json).unwrap();
        let default = FailureLearning::default();
        assert_eq!(fl.enabled, default.enabled);
        assert_eq!(fl.block_threshold, default.block_threshold);
        assert_eq!(fl.window_seconds, default.window_seconds);
        assert_eq!(fl.max_tracked_commands, default.max_tracked_commands);
        assert_eq!(fl.cleanup_after_seconds, default.cleanup_after_seconds);
        assert_eq!(fl.state_file, None);
        assert_eq!(fl.message_template, None);
    }

    #[test]
    fn failure_learning_default_values() {
        let fl = FailureLearning::default();
        assert!(fl.enabled);
        assert_eq!(fl.block_threshold, 3);
        assert_eq!(fl.window_seconds, 300);
        assert_eq!(fl.max_tracked_commands, 200);
        assert_eq!(fl.cleanup_after_seconds, 3600);
    }

    #[test]
    fn rules_config_default_is_empty() {
        let cfg = RulesConfig::default();
        assert!(cfg.rules.is_empty());
        assert!(cfg.failure_learning.enabled);
    }

    #[test]
    fn rules_config_deserializes_full_document() {
        let json = r#"{
            "rules": [
                {"id": "r1", "pattern": "rm -rf"}
            ],
            "failure_learning": {"block_threshold": 5}
        }"#;
        let cfg: RulesConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.rules.len(), 1);
        assert_eq!(cfg.rules[0].id, "r1");
        assert_eq!(cfg.failure_learning.block_threshold, 5);
        // untouched fields still fall back to their own field defaults
        assert_eq!(cfg.failure_learning.window_seconds, 300);
    }

    #[test]
    fn default_helper_functions_return_documented_values() {
        assert!(default_true());
        assert_eq!(default_block_threshold(), 3);
        assert_eq!(default_window(), 300);
        assert_eq!(default_max_entries(), 200);
        assert_eq!(default_cleanup(), 3600);
    }
}
