use crs_core::loader::RulesLoader;
use crs_core::store::StateStore;
use crs_core::{rules, state};

use super::{deny, HookPayload};

/// For the `no-ls-use-glob` rule, extract the target path from the `ls` command and append
/// a file-tree listing so Claude gets useful context without needing to retry.
fn enrich_message(rule_id: &str, command: &str, base_msg: &str) -> String {
    if rule_id != "no-ls-use-glob" {
        return base_msg.to_string();
    }

    // Extract path: take the last whitespace-delimited token that doesn't start with `-`.
    // Falls back to `.` when none is found (bare `ls`).
    let path = command
        .split_whitespace()
        .skip(1) // skip "ls"
        .filter(|t| !t.starts_with('-'))
        .last()
        .unwrap_or(".");

    let tree = file_tree(path);
    format!("{}\n\nDirectory listing for `{}`:\n{}", base_msg, path, tree)
}

/// Run `eza --tree --level 2 <path>`, falling back to a manual two-level walk.
fn file_tree(path: &str) -> String {
    use std::process::Command;

    // Try eza first
    let eza = Command::new("eza")
        .args(["--tree", "--level", "2", "--color=never", path])
        .output();

    if let Ok(out) = eza {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }

    // Fallback: find -maxdepth 2
    let find = Command::new("find")
        .args([path, "-maxdepth", "2", "-not", "-name", ".*"])
        .output();

    if let Ok(out) = find {
        if out.status.success() {
            let mut lines: Vec<_> = String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(str::to_string)
                .collect();
            lines.sort();
            return lines.join("\n");
        }
    }

    "(could not list directory)".to_string()
}

pub fn run_with<L: RulesLoader, S: StateStore>(loader: &L, store: &S, payload: &HookPayload) {
    if payload.tool_name.as_deref() != Some("Bash") {
        return;
    }

    let command = match payload.tool_input.as_ref().and_then(|i| i.command.as_deref()) {
        Some(c) if !c.is_empty() => c,
        _ => return,
    };

    let config = loader.load();
    let fl = &config.failure_learning;

    // 1. Predefined rules
    if let Some((rule_id, msg)) = rules::check(command, &config.rules) {
        if fl.enabled {
            let st = store.load();
            let st = state::record_failure(st, command, fl);
            store.save(&st);
        }
        let full_msg = enrich_message(&rule_id, command, &msg);
        deny(&full_msg);
    }

    // 2. Learned failures
    if fl.enabled {
        let st = store.load();
        if let Some(msg) = state::check_learned(command, &st, fl) {
            deny(&msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crs_core::loader::InMemoryRulesLoader;
    use crs_core::rules::{FailureLearning, Rule, RulesConfig};
    use crs_core::state::{command_key, FailureEntry, State};
    use crs_core::store::InMemoryStateStore;
    use super::super::{HookPayload, ToolInput};
    use std::collections::HashMap;

    fn bash_payload(cmd: &str) -> HookPayload {
        HookPayload {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(ToolInput { command: Some(cmd.to_string()) }),
            tool_response: None,
        }
    }

    fn empty_config() -> RulesConfig {
        RulesConfig {
            rules: vec![],
            failure_learning: FailureLearning::default(),
        }
    }

    fn config_with_rule(pattern: &str) -> RulesConfig {
        RulesConfig {
            rules: vec![Rule {
                id: "test-rule".to_string(),
                enabled: true,
                pattern: pattern.to_string(),
                pattern_flags: String::new(),
                exceptions: vec![],
                message: Some("blocked".to_string()),
            }],
            failure_learning: FailureLearning::default(),
        }
    }

    fn state_with_failures(cmd: &str, count: usize) -> State {
        let now = crs_core::state::now_secs();
        let key = command_key(cmd);
        let mut failures = HashMap::new();
        failures.insert(key, FailureEntry {
            command_preview: cmd.to_string(),
            timestamps: vec![now; count],
            last_seen: now as f64,
        });
        State { failures }
    }

    #[test]
    fn non_bash_tool_passthrough() {
        let loader = InMemoryRulesLoader(config_with_rule(r"\bgrep\b"));
        let store = InMemoryStateStore::new();
        let payload = HookPayload {
            tool_name: Some("Read".to_string()),
            tool_input: None,
            tool_response: None,
        };
        run_with(&loader, &store, &payload);
    }

    #[test]
    fn empty_command_passthrough() {
        let loader = InMemoryRulesLoader(config_with_rule(r"\bgrep\b"));
        let store = InMemoryStateStore::new();
        let payload = HookPayload {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(ToolInput { command: Some(String::new()) }),
            tool_response: None,
        };
        run_with(&loader, &store, &payload);
    }

    #[test]
    fn allowed_command_completes() {
        let loader = InMemoryRulesLoader(config_with_rule(r"\bgrep\b"));
        let store = InMemoryStateStore::new();
        run_with(&loader, &store, &bash_payload("ls -la"));
    }

    #[test]
    fn rule_block_records_failure_in_state() {
        use crs_core::rules::{FailureLearning, Rule, RulesConfig};
        use crs_core::state;

        let config = RulesConfig {
            rules: vec![Rule {
                id: "test-rule".to_string(),
                enabled: true,
                pattern: r"\bgrep\b".to_string(),
                pattern_flags: String::new(),
                exceptions: vec![],
                message: Some("blocked".to_string()),
            }],
            failure_learning: FailureLearning {
                enabled: true,
                block_threshold: 3,
                window_seconds: 300,
                state_file: None,
                max_tracked_commands: 200,
                cleanup_after_seconds: 3600,
                message_template: None,
            },
        };
        let store = InMemoryStateStore::new();
        let fl = &config.failure_learning;
        let command = "grep foo .";

        // Simulate what run_with does on a rule match — record before deny
        for _ in 0..2 {
            let st = store.load();
            let st = state::record_failure(st, command, fl);
            store.save(&st);
        }

        assert_eq!(
            store.get_state().failures.values().next().unwrap().timestamps.len(),
            2
        );
    }

    #[test]
    fn failure_learning_disabled_allows_at_threshold() {
        let mut config = empty_config();
        config.failure_learning.enabled = false;
        let loader = InMemoryRulesLoader(config);
        let store = InMemoryStateStore::with_state(state_with_failures("grep foo .", 5));
        run_with(&loader, &store, &bash_payload("grep foo ."));
    }
}

pub fn run() {
    use crs_core::loader::FsRulesLoader;
    use crs_core::state::state_path;
    use crs_core::store::FsStateStore;

    let Some(payload) = super::read_stdin() else {
        return;
    };

    let loader = FsRulesLoader;
    let config = loader.load();
    let path = state_path(&config.failure_learning);
    let store = FsStateStore { path };

    run_with(&loader, &store, &payload);
}
