use crs_core::capture::CaptureStore;
use crs_core::loader::RulesLoader;
use crs_core::store::StateStore;
use crs_core::{rules, state};

use super::{HookPayload, deny_with_protocol};
use crs_core::config::HookProtocol;

/// Returns true when `rule_id` is the ls rule that warrants directory enrichment.
pub(crate) fn should_enrich(rule_id: &str) -> bool {
    rule_id == "no-ls-use-glob"
}

/// Extract the target path from an `ls` command.
/// Takes the last non-flag token after `ls`; falls back to `.` for bare `ls`.
pub(crate) fn extract_ls_path(command: &str) -> &str {
    command
        .split_whitespace()
        .skip(1) // skip "ls"
        .filter(|t| !t.starts_with('-'))
        .last()
        .unwrap_or(".")
}

// qual:allow(iosp) reason: "integration glue — dispatches to file_tree I/O"
fn enrich_message(rule_id: &str, command: &str, base_msg: &str) -> String {
    if !should_enrich(rule_id) {
        return base_msg.to_string();
    }

    let path = extract_ls_path(command);
    let tree = file_tree(path);
    format!(
        "{}\n\nDirectory listing for `{}`:\n{}",
        base_msg, path, tree
    )
}

// qual:allow(iosp) reason: "I/O boundary — spawns eza/find subprocesses"
fn file_tree(path: &str) -> String {
    use std::process::Command;

    // Try eza first
    let eza = Command::new("eza")
        .args(["--tree", "--level", "2", "--color=never", path])
        .output();

    if let Ok(out) = eza
        && out.status.success()
    {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() {
            return s;
        }
    }

    // Fallback: find -maxdepth 2
    let find = Command::new("find")
        .args([path, "-maxdepth", "2", "-not", "-name", ".*"])
        .output();

    if let Ok(out) = find
        && out.status.success()
    {
        let mut lines: Vec<_> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect();
        lines.sort();
        return lines.join("\n");
    }

    "(could not list directory)".to_string()
}

// TODO(hook-ordering-semantics): document whether Claude Code short-circuits on
// the first deny response from a hook chain or runs all hooks in the chain.
// If short-circuit: the failure-learning check below (step 2) is skipped when a
// rule fires (step 1). If not: both can produce output. Clarify in CLAUDE.md.
//
// TODO(no-sed-n-use-read): enable the `no-sed-n-use-read` block rule once the
// Read tool's offset/limit feature is stable. Currently deferred because the
// alternative (Read with offset) is not yet ergonomic enough to enforce.
//
// TODO(coursers-11): coursers-11 (cross-tool block) depends on obfsck-11 and
// mcpipe-21 in external repos. No local fallback is documented. When those issues
// are resolved, wire the cross-tool detection here.
/// Core pre-hook logic, injectable for testing.
///
/// Blocks the command with a deny response if it matches a rule and no exception applies.
// qual:allow(iosp) reason: "integration root — orchestrates rule checks"
pub fn run_with<L: RulesLoader, S: StateStore>(
    loader: &L,
    store: &S,
    capture: &dyn CaptureStore,
    payload: &HookPayload,
) {
    run_with_proto(loader, store, capture, payload, HookProtocol::Claude);
}

/// Core pre-hook logic with explicit protocol selection.
// qual:allow(iosp) reason: "integration root -- orchestrates rule checks"
pub fn run_with_proto<L: RulesLoader, S: StateStore>(
    loader: &L,
    store: &S,
    capture: &dyn CaptureStore,
    payload: &HookPayload,
    protocol: HookProtocol,
) {
    if payload.tool_name.as_deref() != Some("Bash") {
        return;
    }

    let command = match payload
        .tool_input
        .as_ref()
        .and_then(|i| i.command.as_deref())
    {
        Some(c) if !c.is_empty() => c,
        _ => return,
    };

    let config = loader.load().unwrap_or_else(|e| {
        eprintln!("[coursers] warning: failed to load rules: {e}");
        crs_core::rules::RulesConfig {
            rules: vec![],
            failure_learning: crs_core::rules::FailureLearning::default(),
        }
    });
    let fl = &config.failure_learning;

    // 1. Predefined rules
    if let Some((rule_id, msg)) = rules::check_pipeline(command, &config.rules) {
        crs_core::stats::record_block(&crs_core::stats::stats_path(), &rule_id);

        // Capture (original, suggestion) pair for fine-tuning dataset.
        let cwd = payload
            .cwd
            .clone()
            .or_else(|| {
                std::env::current_dir()
                    .ok()
                    .map(|p| p.display().to_string())
            })
            .unwrap_or_default();
        capture
            .record(crs_core::capture::SuggestionRecord::new(
                command,
                &msg,
                &rule_id,
                cwd,
                payload.session_id.clone(),
                payload.tool_name.as_deref().unwrap_or("Bash"),
            ))
            .unwrap_or_else(|e| eprintln!("[coursers] warning: failed to record suggestion: {e}"));

        let full_msg = enrich_message(&rule_id, command, &msg);
        deny_with_protocol(protocol, &full_msg);
    }

    // 2. Learned failures
    if fl.enabled {
        let st = store.load().unwrap_or_else(|e| {
            eprintln!("[coursers] warning: failed to load state: {e}");
            crs_core::state::State::default()
        });
        if let Some(msg) = state::check_learned(command, &st, fl) {
            deny_with_protocol(protocol, &msg);
        }
    }
}

/// Default entry point for the `coursers pre` hook.
#[allow(dead_code)]
pub fn run() {
    let Some((payload, loader, store, capture)) = super::hook_context() else {
        return;
    };
    run_with(&loader, &store, &capture, &payload);
}

/// Profile-aware entry point for `coursers pre --profile <name>`.
pub fn run_with_profile(profile_cfg: &crs_core::config::ProfileConfig) {
    let Some((payload, loader, store, capture)) = super::hook_context_with_profile(profile_cfg)
    else {
        return;
    };
    run_with_proto(&loader, &store, &capture, &payload, profile_cfg.protocol);
}

#[cfg(test)]
mod tests {
    use super::super::{HookPayload, ToolInput};
    use super::*;
    use crs_core::capture::InMemoryCaptureStore;
    use crs_core::loader::InMemoryRulesLoader;
    use crs_core::rules::{FailureLearning, Rule, RulesConfig};
    use crs_core::state::{FailureEntry, State, command_key};
    use crs_core::store::InMemoryStateStore;
    use std::collections::HashMap;

    fn bash_payload(cmd: &str) -> HookPayload {
        HookPayload {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(ToolInput {
                command: Some(cmd.to_string()),
            }),
            tool_response: None,
            session_id: None,
            cwd: None,
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
                target_commands: vec![],
                message: Some("blocked".to_string()),
            }],
            failure_learning: FailureLearning::default(),
        }
    }

    fn state_with_failures(cmd: &str, count: usize) -> State {
        let now = crs_core::state::now_secs();
        let key = command_key(cmd);
        let mut failures = HashMap::new();
        failures.insert(
            key,
            FailureEntry {
                command_preview: cmd.to_string(),
                timestamps: vec![now; count],
                last_seen: now as f64,
            },
        );
        State { failures }
    }

    #[test]
    fn should_enrich_true_for_ls_rule() {
        assert!(should_enrich("no-ls-use-glob"));
    }

    #[test]
    fn should_enrich_false_for_other_rules() {
        assert!(!should_enrich("no-find-use-glob"));
        assert!(!should_enrich(""));
    }

    #[test]
    fn extract_ls_path_bare_ls() {
        assert_eq!(extract_ls_path("ls"), ".");
    }

    #[test]
    fn extract_ls_path_with_flag_only() {
        assert_eq!(extract_ls_path("ls -la"), ".");
    }

    #[test]
    fn extract_ls_path_with_target() {
        assert_eq!(extract_ls_path("ls -la /tmp"), "/tmp");
    }

    #[test]
    fn extract_ls_path_last_non_flag_wins() {
        assert_eq!(extract_ls_path("ls src/ tests/"), "tests/");
    }

    #[test]
    fn non_bash_tool_passthrough() {
        let loader = InMemoryRulesLoader(config_with_rule(r"\bgrep\b"));
        let store = InMemoryStateStore::new();
        let payload = HookPayload {
            tool_name: Some("Read".to_string()),
            tool_input: None,
            tool_response: None,
            session_id: None,
            cwd: None,
        };
        run_with(&loader, &store, &InMemoryCaptureStore::new(), &payload);
    }

    #[test]
    fn empty_command_passthrough() {
        let loader = InMemoryRulesLoader(config_with_rule(r"\bgrep\b"));
        let store = InMemoryStateStore::new();
        let payload = HookPayload {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(ToolInput {
                command: Some(String::new()),
            }),
            tool_response: None,
            session_id: None,
            cwd: None,
        };
        run_with(&loader, &store, &InMemoryCaptureStore::new(), &payload);
    }

    #[test]
    fn allowed_command_completes() {
        let loader = InMemoryRulesLoader(config_with_rule(r"\bgrep\b"));
        let store = InMemoryStateStore::new();
        run_with(
            &loader,
            &store,
            &InMemoryCaptureStore::new(),
            &bash_payload("ls -la"),
        );
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
                target_commands: vec![],
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
            let st = store.load().unwrap_or_default();
            let st = state::record_failure(st, command, fl);
            store.save(&st).unwrap();
        }

        assert_eq!(
            store
                .get_state()
                .failures
                .values()
                .next()
                .unwrap()
                .timestamps
                .len(),
            2
        );
    }

    #[test]
    fn failure_learning_disabled_allows_at_threshold() {
        let mut config = empty_config();
        config.failure_learning.enabled = false;
        let loader = InMemoryRulesLoader(config);
        let store = InMemoryStateStore::with_state(state_with_failures("grep foo .", 5));
        run_with(
            &loader,
            &store,
            &InMemoryCaptureStore::new(),
            &bash_payload("grep foo ."),
        );
    }
}
