use crs_core::capture::CaptureStore;
use crs_core::loader::RulesLoader;
use crs_core::state;
use crs_core::store::StateStore;

use super::HookPayload;

const SIGNAL_EXIT_CODES: &[i64] = &[130, 137, 143];

static EXCLUDE_RES: std::sync::LazyLock<Vec<regex::Regex>> = std::sync::LazyLock::new(|| {
    [
        r"^\s*false\s*$",
        r"\|\|\s*(true|:)\s*$",
        r";\s*(true|:)\s*$",
        r"^\s*\[",
        r"\btest\s+-[defhlrswxz]\b",
        r"2>/dev/null",
        r">/dev/null\s+2>&1",
    ]
    .iter()
    .map(|p| regex::Regex::new(p).expect("hardcoded exclude pattern is valid"))
    .collect()
});

fn is_excluded(command: &str) -> bool {
    EXCLUDE_RES.iter().any(|re| re.is_match(command))
}

/// Pure predicate: true when a failure for `command` with `exit_code` should be recorded.
/// Does not check whether failure_learning is enabled — that is the caller's concern.
pub(crate) fn should_record(exit_code: i64, command: &str) -> bool {
    if exit_code == 0 {
        return false;
    }
    if SIGNAL_EXIT_CODES.contains(&exit_code) {
        return false;
    }
    !is_excluded(command)
}

pub fn run_with<L: RulesLoader, S: StateStore>(
    loader: &L,
    store: &S,
    capture: &dyn CaptureStore,
    payload: &HookPayload,
) {
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
            capture
                .mark_accepted(session_id, command, exit_code)
                .unwrap_or_else(|e| eprintln!("[coursers] warning: failed to mark accepted: {e}"));
        }
        return;
    }

    if !should_record(exit_code, command) {
        return;
    }

    let config = loader.load().unwrap_or_else(|e| {
        eprintln!("[coursers] warning: failed to load rules: {e}");
        crs_core::rules::RulesConfig {
            rules: vec![],
            failure_learning: crs_core::rules::FailureLearning::default(),
        }
    });
    let fl = &config.failure_learning;
    if !fl.enabled {
        return;
    }

    let st = store.load().unwrap_or_else(|e| {
        eprintln!("[coursers] warning: failed to load state: {e}");
        crs_core::state::State::default()
    });
    let st = state::record_failure(st, command, fl);
    store
        .save(&st)
        .unwrap_or_else(|e| eprintln!("[coursers] warning: failed to save state: {e}"));
}

#[allow(dead_code)]
pub fn run() {
    let Some((payload, loader, store, capture)) = super::hook_context() else {
        return;
    };
    run_with(&loader, &store, &capture, &payload);
}

/// Profile-aware entry point for `coursers post --profile <name>`.
pub fn run_with_profile(profile_cfg: &crs_core::config::ProfileConfig) {
    let Some((payload, loader, store, capture)) = super::hook_context_with_profile(profile_cfg)
    else {
        return;
    };
    run_with(&loader, &store, &capture, &payload);
}

#[cfg(test)]
mod tests {
    use super::super::{HookPayload, ToolInput};
    use super::*;
    use crs_core::capture::InMemoryCaptureStore;
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
    fn should_record_exit_zero_is_false() {
        assert!(!should_record(0, "grep foo ."));
    }

    #[test]
    fn should_record_signal_exits_are_false() {
        assert!(!should_record(130, "grep foo ."));
        assert!(!should_record(137, "grep foo ."));
        assert!(!should_record(143, "grep foo ."));
    }

    #[test]
    fn should_record_excluded_pattern_is_false() {
        assert!(!should_record(1, "cmd 2>/dev/null"));
    }

    #[test]
    fn should_record_real_failure_is_true() {
        assert!(should_record(1, "grep foo ."));
    }

    #[test]
    fn exit_zero_no_record() {
        let loader = InMemoryRulesLoader(config_fl(true));
        let store = InMemoryStateStore::new();
        run_with(
            &loader,
            &store,
            &InMemoryCaptureStore::new(),
            &post_payload("grep foo .", 0),
        );
        assert!(store.get_state().failures.is_empty());
    }

    #[test]
    fn signal_exit_no_record() {
        let loader = InMemoryRulesLoader(config_fl(true));
        let store = InMemoryStateStore::new();
        run_with(
            &loader,
            &store,
            &InMemoryCaptureStore::new(),
            &post_payload("grep foo .", 130),
        );
        assert!(store.get_state().failures.is_empty());
    }

    #[test]
    fn excluded_pattern_no_record() {
        let loader = InMemoryRulesLoader(config_fl(true));
        let store = InMemoryStateStore::new();
        run_with(
            &loader,
            &store,
            &InMemoryCaptureStore::new(),
            &post_payload("cmd 2>/dev/null", 1),
        );
        assert!(store.get_state().failures.is_empty());
    }

    #[test]
    fn real_failure_recorded() {
        let loader = InMemoryRulesLoader(config_fl(true));
        let store = InMemoryStateStore::new();
        run_with(
            &loader,
            &store,
            &InMemoryCaptureStore::new(),
            &post_payload("grep foo .", 1),
        );
        assert!(!store.get_state().failures.is_empty());
    }

    #[test]
    fn failure_learning_disabled_no_record() {
        let loader = InMemoryRulesLoader(config_fl(false));
        let store = InMemoryStateStore::new();
        run_with(
            &loader,
            &store,
            &InMemoryCaptureStore::new(),
            &post_payload("grep foo .", 1),
        );
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
        run_with(&loader, &store, &InMemoryCaptureStore::new(), &payload);
        assert!(store.get_state().failures.is_empty());
    }
}
