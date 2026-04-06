#[path = "common.rs"]
mod common;

use common::{fixture, run_post};
use tempfile::TempDir;

#[test]
fn failure_recorded_in_state() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state.json");
    run_post(
        &fixture("payload_post_fail.json"),
        &fixture("rules_basic.json"),
        &state,
    );
    assert!(state.exists(), "state file should be created after failure");
    let content = std::fs::read_to_string(&state).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(
        !parsed["failures"]
            .as_object()
            .unwrap_or(&serde_json::Map::new())
            .is_empty(),
        "failures should not be empty"
    );
}

#[test]
fn success_not_recorded() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state.json");
    run_post(
        &fixture("payload_post_ok.json"),
        &fixture("rules_basic.json"),
        &state,
    );
    assert!(
        !state.exists(),
        "state file should not be created for success"
    );
}

#[test]
fn signal_not_recorded() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state.json");
    run_post(
        &fixture("payload_post_signal.json"),
        &fixture("rules_basic.json"),
        &state,
    );
    assert!(
        !state.exists(),
        "state file should not be created for signal exit"
    );
}

#[test]
fn excluded_pattern_not_recorded() {
    let tmp = TempDir::new().unwrap();
    let state_path = tmp.path().join("state.json");
    let payload_path = tmp.path().join("payload_excluded.json");
    std::fs::write(
        &payload_path,
        r#"{
        "tool_name": "Bash",
        "tool_input": { "command": "cmd 2>/dev/null" },
        "tool_response": { "exit_code": 1 }
    }"#,
    )
    .unwrap();
    run_post(&payload_path, &fixture("rules_basic.json"), &state_path);
    assert!(
        !state_path.exists(),
        "excluded pattern should not be recorded"
    );
}
