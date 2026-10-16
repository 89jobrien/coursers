//! Integration tests for PreToolUse rule enforcement.

#[path = "common.rs"]
mod common;

use common::{fixture, run_hook, run_post, run_pre};
use tempfile::{NamedTempFile, TempDir};

fn example_rules_with_sed_rule_enabled() -> NamedTempFile {
    let rules_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../config/course-correct-rules.example.json");
    let mut rules: serde_json::Value =
        serde_json::from_slice(&std::fs::read(rules_path).unwrap()).unwrap();
    {
        let rule_entries = rules["rules"].as_array_mut().unwrap();
        for rule in &mut *rule_entries {
            rule["enabled"] = false.into();
        }
        rule_entries
            .iter_mut()
            .find(|rule| rule["id"] == "no-sed-n-use-read")
            .unwrap()["enabled"] = true.into();
    }

    let file = NamedTempFile::new().unwrap();
    serde_json::to_writer(file.as_file(), &rules).unwrap();
    file
}

#[test]
fn blocked_command_exits_nonzero() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state.json");
    let out = run_pre(
        &fixture("payload_bash_grep.json"),
        &fixture("rules_basic.json"),
        &state,
    );
    assert!(
        !out.status.success(),
        "expected non-zero exit, got: {:?}",
        out.status
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("block") || stdout.contains("deny"),
        "expected 'block' or 'deny' in stdout, got: {stdout}"
    );
}

#[test]
fn predefined_rule_deny_short_circuits_learned_failure_check() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state.json");
    let rules = fixture("rules_basic.json");

    for _ in 0..3 {
        let out = run_post(&fixture("payload_post_fail.json"), &rules, &state);
        assert!(out.status.success(), "failed to seed failure state");
    }

    let out = run_pre(&fixture("payload_bash_grep.json"), &rules, &state);
    assert_eq!(out.status.code(), Some(2));

    let response: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("pre hook must emit exactly one JSON response");
    let reason = response["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .expect("deny response must include a reason");
    assert!(reason.contains("Grep tool"), "unexpected reason: {reason}");
    assert!(
        !reason.contains("exact command has failed"),
        "learned-failure deny must not replace the earlier rule deny: {reason}"
    );
}

#[test]
fn allowed_command_exits_zero() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state.json");
    let out = run_pre(
        &fixture("payload_bash_ls.json"),
        &fixture("rules_basic.json"),
        &state,
    );
    assert!(
        out.status.success(),
        "expected exit 0, got: {:?}",
        out.status
    );
}

#[test]
fn non_bash_passthrough() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state.json");
    let out = run_pre(
        &fixture("payload_non_bash.json"),
        &fixture("rules_basic.json"),
        &state,
    );
    assert!(
        out.status.success(),
        "expected exit 0 for non-Bash tool, got: {:?}",
        out.status
    );
}

#[test]
fn sed_line_range_rule_denies_with_protocol_safe_json() {
    let tmp = TempDir::new().unwrap();
    let rules = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../config/course-correct-rules.example.json");
    let payload = serde_json::json!({
        "tool_name": "Bash",
        "tool_input": { "command": "sed -n '10,20p' src/main.rs" }
    });

    let out = run_hook(
        "pre",
        &payload.to_string(),
        &rules,
        &tmp.path().join("state.json"),
    );

    assert_eq!(out.status.code(), Some(2));
    let response: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("deny response must be exactly one JSON value");
    assert_eq!(response["hookSpecificOutput"]["permissionDecision"], "deny");
    assert!(
        response["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap()
            .contains("offset=N, limit=N")
    );
}

#[test]
fn sed_line_range_matcher_allows_other_numeric_sed_programs() {
    let tmp = TempDir::new().unwrap();
    let rules = example_rules_with_sed_rule_enabled();
    let payload = serde_json::json!({
        "tool_name": "Bash",
        "tool_input": { "command": "sed -n 10s/foo/bar/p src/main.rs" }
    });

    let out = run_hook(
        "pre",
        &payload.to_string(),
        rules.path(),
        &tmp.path().join("state.json"),
    );

    assert!(
        out.status.success(),
        "non-range sed program must pass; stdout: {}; stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn learned_failure_blocks_after_threshold() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state.json");
    let rules = fixture("rules_empty.json");

    // Record 3 failures via post
    for _ in 0..3 {
        run_post(&fixture("payload_post_fail.json"), &rules, &state);
    }

    // Now pre should block the same command
    let out = run_pre(&fixture("payload_bash_grep.json"), &rules, &state);
    assert!(!out.status.success(), "expected block after 3 failures");
}
