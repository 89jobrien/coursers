use regex::Regex;
use serde::Deserialize;
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

/// Returns the deny message if any rule matches, None otherwise.
pub fn check(command: &str, rules: &[Rule]) -> Option<String> {
    for rule in rules {
        if !rule.enabled {
            continue;
        }

        let pattern_str = if rule.pattern_flags.contains('i') || rule.pattern.contains("(?i)") {
            format!("(?i){}", rule.pattern)
        } else {
            rule.pattern.clone()
        };

        let Ok(re) = Regex::new(&pattern_str) else {
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

        let msg = rule.message.clone().unwrap_or_else(|| {
            format!("Blocked by rule '{}'.", rule.id)
        });
        return Some(msg);
    }
    None
}
