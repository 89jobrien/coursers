use crs_core::loader::RulesLoader;
use crs_core::state;
use crs_core::store::StateStore;

use super::HookPayload;

const SIGNAL_EXIT_CODES: &[i64] = &[130, 137, 143];
const EXCLUDE_PATTERNS: &[&str] = &[
    r"^\s*false\s*$",
    r"\|\|\s*(true|:)\s*$",
    r";\s*(true|:)\s*$",
    r"^\s*\[",
    r"\btest\s+-[defhlrswxz]\b",
    r"2>/dev/null",
    r">/dev/null\s+2>&1",
];

pub fn run_with<L: RulesLoader, S: StateStore>(loader: &L, store: &S, payload: &HookPayload) {
    if payload.tool_name.as_deref() != Some("Bash") {
        return;
    }

    let exit_code = payload
        .tool_response
        .as_ref()
        .and_then(|r| r.get("exit_code"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let command = match payload
        .tool_input
        .as_ref()
        .and_then(|i| i.command.as_deref())
    {
        Some(c) if !c.is_empty() => c,
        _ => return,
    };

    // Check for suggestion acceptance on exit 0.
    if exit_code == 0 {
        if let Some(session_id) = payload.session_id.as_deref() {
            let capture_store = crs_core::capture::SuggestionStore::new(
                crs_core::capture::SuggestionStore::default_path(),
            );
            capture_store.mark_accepted(session_id, command, exit_code);
        }
        return;
    }

    if SIGNAL_EXIT_CODES.contains(&exit_code) {
        return;
    }

    if is_excluded(command) {
        return;
    }

    let config = loader.load();
    let fl = &config.failure_learning;
    if !fl.enabled {
        return;
    }

    let st = store.load();
    let st = state::record_failure(st, command, fl);
    store.save(&st);
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

#[cfg(test)]
mod tests {
    use super::super::{HookPayload, ToolInput};
    use super::*;
    use crs_core::loader::InMemoryRulesLoader;
    use crs_core::rules::{FailureLearning, RulesConfig};
    use crs_core::store::InMemoryStateStore;
    use serde_json::json;

    fn config_fl(enabled: bool) -> RulesConfig {
        RulesConfig {
            rules: vec![],
            failure_learning: FailureLearning {
                enabled,
                block_threshold: 3,
                window_seconds: 300,
                state_file: None,
                max_tracked_commands: 200,
                cleanup_after_seconds: 3600,
                message_template: None,
            },
        }
    }

    fn post_payload(cmd: &str, exit_code: i64) -> HookPayload {
        HookPayload {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(ToolInput {
                command: Some(cmd.to_string()),
            }),
            tool_response: Some(json!({ "exit_code": exit_code })),
            session_id: None,
            cwd: None,
        }
    }

    #[test]
    fn exit_zero_no_record() {
        let loader = InMemoryRulesLoader(config_fl(true));
        let store = InMemoryStateStore::new();
        run_with(&loader, &store, &post_payload("grep foo .", 0));
        assert!(store.get_state().failures.is_empty());
    }

    #[test]
    fn signal_exit_no_record() {
        let loader = InMemoryRulesLoader(config_fl(true));
        let store = InMemoryStateStore::new();
        run_with(&loader, &store, &post_payload("grep foo .", 130));
        assert!(store.get_state().failures.is_empty());
    }

    #[test]
    fn excluded_pattern_no_record() {
        let loader = InMemoryRulesLoader(config_fl(true));
        let store = InMemoryStateStore::new();
        run_with(&loader, &store, &post_payload("cmd 2>/dev/null", 1));
        assert!(store.get_state().failures.is_empty());
    }

    #[test]
    fn real_failure_recorded() {
        let loader = InMemoryRulesLoader(config_fl(true));
        let store = InMemoryStateStore::new();
        run_with(&loader, &store, &post_payload("grep foo .", 1));
        assert!(!store.get_state().failures.is_empty());
    }

    #[test]
    fn failure_learning_disabled_no_record() {
        let loader = InMemoryRulesLoader(config_fl(false));
        let store = InMemoryStateStore::new();
        run_with(&loader, &store, &post_payload("grep foo .", 1));
        assert!(store.get_state().failures.is_empty());
    }

    #[test]
    fn non_bash_tool_no_record() {
        let loader = InMemoryRulesLoader(config_fl(true));
        let store = InMemoryStateStore::new();
        let payload = HookPayload {
            tool_name: Some("Read".to_string()),
            tool_input: Some(ToolInput {
                command: Some("grep foo .".to_string()),
            }),
            tool_response: Some(json!({ "exit_code": 1 })),
            session_id: None,
            cwd: None,
        };
        run_with(&loader, &store, &payload);
        assert!(store.get_state().failures.is_empty());
    }
}

fn is_excluded(command: &str) -> bool {
    EXCLUDE_PATTERNS.iter().any(|pat| {
        regex::Regex::new(pat)
            .map(|re| re.is_match(command))
            .unwrap_or(false)
    })
}
