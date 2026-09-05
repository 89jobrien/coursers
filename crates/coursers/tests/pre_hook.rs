#[path = "common.rs"]
mod common;

use common::{fixture, run_post, run_pre};
use tempfile::TempDir;

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
