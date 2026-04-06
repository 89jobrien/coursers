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
fn allowed_command_exits_zero() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state.json");
    let out = run_pre(
        &fixture("payload_bash_ls.json"),
        &fixture("rules_basic.json"),
        &state,
    );
    assert!(out.status.success(), "expected exit 0, got: {:?}", out.status);
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
