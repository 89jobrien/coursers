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
}
