use crs_core::loader::RulesLoader;
use crs_core::store::StateStore;
use crs_core::{rules, state};

use super::{deny, HookPayload};

pub fn run_with<L: RulesLoader, S: StateStore>(loader: &L, store: &S, payload: &HookPayload) {
    if payload.tool_name.as_deref() != Some("Bash") {
        return;
    }

    let command = match payload.tool_input.as_ref().and_then(|i| i.command.as_deref()) {
        Some(c) if !c.is_empty() => c,
        _ => return,
    };

    let config = loader.load();

    // 1. Predefined rules
    if let Some(msg) = rules::check(command, &config.rules) {
        deny(&msg);
    }

    // 2. Learned failures
    let fl = &config.failure_learning;
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
