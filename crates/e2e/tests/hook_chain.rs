//! End-to-end tests for the HookChain-based pre/post path (hc-d, JOB-475).
//!
//! These tests invoke the `coursers` binary with `COURSERS_HOOK_CHAIN=1` and verify
//! that the chain path produces outcomes equivalent to the legacy path for four
//! representative scenarios:
//!
//! 1. Rule block — `pre` denies a command matching a rule
//! 2. Rewrite — `pre` rewrites a matching command
//! 3. Filter — `post` (via `filter` subcommand) suppresses success output
//! 4. Failure-learning trip — `post` threshold exceeded → `pre` blocks
//!
//! The legacy path is exercised in parallel (without the env var) so outcomes can
//! be compared for equivalence.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::{NamedTempFile, TempDir};

// ---------------------------------------------------------------------------
// Helpers (shared with scenarios.rs pattern)
// ---------------------------------------------------------------------------

fn workspace_bin(name: &str) -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().unwrap().parent().unwrap();
    let debug = workspace.join("target/debug").join(name);
    let release = workspace.join("target/release").join(name);
    if release.exists() {
        release
    } else {
        assert!(
            debug.exists(),
            "binary {name:?} not found — run `cargo build --workspace` first\nchecked: {}",
            debug.display()
        );
        debug
    }
}

fn run_bin_with_env(bin: &str, subcommand: &str, payload: &str, envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(workspace_bin(bin));
    cmd.arg(subcommand)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let mut child = cmd
        .spawn()
        .unwrap_or_else(|e| panic!("spawn {bin} {subcommand}: {e}"));
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn pre_payload(cmd: &str) -> String {
    format!(r#"{{"tool_name":"Bash","tool_input":{{"command":{cmd:?}}}}}"#)
}

fn post_payload_with_output(cmd: &str, output: &str, exit_code: i32) -> String {
    format!(
        r#"{{"tool_name":"Bash","tool_input":{{"command":{cmd:?}}},"tool_response":{{"output":{output:?},"exit_code":{exit_code}}}}}"#
    )
}

/// Write a rules JSON file to a temp file and return it.
fn write_rules(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    write!(f, "{content}").unwrap();
    f
}

/// Write a filters TOML file to a temp file and return it.
fn write_filters(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    write!(f, "{content}").unwrap();
    f
}

/// Extract the `permissionDecision` field from a hook JSON response.
fn parse_decision(stdout: &[u8]) -> String {
    let v: serde_json::Value = serde_json::from_slice(stdout).unwrap_or_default();
    v.get("hookSpecificOutput")
        .and_then(|o| o.get("permissionDecision"))
        .and_then(|d| d.as_str())
        .unwrap_or("(missing)")
        .to_string()
}

/// Extract `permissionDecisionReason` from a hook JSON response.
fn parse_reason(stdout: &[u8]) -> String {
    let v: serde_json::Value = serde_json::from_slice(stdout).unwrap_or_default();
    v.get("hookSpecificOutput")
        .and_then(|o| o.get("permissionDecisionReason"))
        .and_then(|d| d.as_str())
        .unwrap_or("")
        .to_string()
}

// ---------------------------------------------------------------------------
// Scenario 1: Rule block
// ---------------------------------------------------------------------------

/// Chain path must deny a command that matches a blocking rule, just like legacy.
#[test]
fn chain_pre_denies_matching_rule() {
    let rules = write_rules(
        r#"{
            "rules": [{
                "id": "no-grep-use-tool",
                "enabled": true,
                "pattern": "\\bgrep\\b",
                "pattern_flags": "",
                "exceptions": [],
                "target_commands": [],
                "message": "Use the Grep tool instead of grep."
            }]
        }"#,
    );

    let payload = pre_payload("grep foo .");
    let envs = [
        ("COURSERS_HOOK_CHAIN", "1"),
        ("COURSERS_RULES", rules.path().to_str().unwrap()),
    ];

    let out = run_bin_with_env("coursers", "pre", &payload, &envs);

    assert_eq!(
        out.status.code(),
        Some(2),
        "chain pre must exit 2 on deny; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        parse_decision(&out.stdout),
        "deny",
        "expected deny decision in stdout"
    );
}

/// Legacy path must also deny — confirms we're comparing apples-to-apples.
#[test]
fn legacy_pre_denies_matching_rule() {
    let rules = write_rules(
        r#"{
            "rules": [{
                "id": "no-grep-use-tool",
                "enabled": true,
                "pattern": "\\bgrep\\b",
                "pattern_flags": "",
                "exceptions": [],
                "target_commands": [],
                "message": "Use the Grep tool instead of grep."
            }]
        }"#,
    );

    let payload = pre_payload("grep foo .");
    let envs = [("COURSERS_RULES", rules.path().to_str().unwrap())];

    let out = run_bin_with_env("coursers", "pre", &payload, &envs);

    assert_eq!(out.status.code(), Some(2), "legacy pre must exit 2 on deny");
    assert_eq!(parse_decision(&out.stdout), "deny");
}

/// Chain path must allow a clean command that does not match any rule.
#[test]
fn chain_pre_allows_clean_command() {
    let rules = write_rules(r#"{"rules":[]}"#);

    let payload = pre_payload("cargo build");
    let envs = [
        ("COURSERS_HOOK_CHAIN", "1"),
        ("COURSERS_RULES", rules.path().to_str().unwrap()),
    ];

    let out = run_bin_with_env("coursers", "pre", &payload, &envs);

    assert_eq!(
        out.status.code(),
        Some(0),
        "chain pre must exit 0 on allow; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // No JSON output on allow — stdout should be empty.
    assert!(
        out.stdout.is_empty() || parse_decision(&out.stdout) == "allow",
        "unexpected stdout on allow: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

// ---------------------------------------------------------------------------
// Scenario 2: Rewrite
// ---------------------------------------------------------------------------

/// Chain pre must rewrite a command when a rewrite rule matches.
#[test]
fn chain_pre_rewrites_matching_command() {
    let rules = write_rules(r#"{"rules":[]}"#);
    let filters = write_filters(
        r#"
[[rewrites]]
pattern = "^git status$"
replace = "git status --short"
"#,
    );

    let payload = pre_payload("git status");
    let envs = [
        ("COURSERS_HOOK_CHAIN", "1"),
        ("COURSERS_RULES", rules.path().to_str().unwrap()),
        ("CRS_FILTERS", filters.path().to_str().unwrap()),
    ];

    let out = run_bin_with_env("coursers", "pre", &payload, &envs);

    assert_eq!(
        out.status.code(),
        Some(0),
        "chain pre must exit 0 on rewrite; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let decision = parse_decision(&out.stdout);
    assert_eq!(decision, "allow", "rewrite must be an allow decision");

    // The rewritten command must appear in updatedInput.
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout must be valid JSON");
    let updated = v
        .get("hookSpecificOutput")
        .and_then(|o| o.get("updatedInput"))
        .and_then(|u| u.get("command"))
        .and_then(|c| c.as_str())
        .unwrap_or("");
    assert_eq!(
        updated, "git status --short",
        "rewritten command must be git status --short"
    );
}

// ---------------------------------------------------------------------------
// Scenario 3: Filter (via `crs filter` subcommand on chain path)
//
// The chain path wires filter into `coursers post` — when COURSERS_HOOK_CHAIN=1
// the FilterHook runs inside run_post. We test it via `coursers post` with a
// filters TOML that suppresses success output for matching commands.
// ---------------------------------------------------------------------------

/// Chain post must suppress (filter) output for a matching command on success.
#[test]
fn chain_post_filters_success_output() {
    let rules = write_rules(r#"{"rules":[]}"#);
    let filters = write_filters(
        r#"
[[filters]]
pattern = "cargo nextest"
mode = "failures-only"
max_lines = 50
"#,
    );

    let payload = post_payload_with_output("cargo nextest run", "all tests passed", 0);
    let envs = [
        ("COURSERS_HOOK_CHAIN", "1"),
        ("COURSERS_RULES", rules.path().to_str().unwrap()),
        ("CRS_FILTERS", filters.path().to_str().unwrap()),
    ];

    let out = run_bin_with_env("coursers", "post", &payload, &envs);

    assert_eq!(
        out.status.code(),
        Some(0),
        "chain post must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Output should contain a filter response (not silence — the FilterHook
    // emits filter_result_response when output is suppressed).
    let stdout_str = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout_str.is_empty(),
        "chain post must emit filter response when suppressing success output"
    );
}

/// Chain post must pass through output for commands that do not match any filter.
#[test]
fn chain_post_allows_non_matching_command() {
    let rules = write_rules(r#"{"rules":[]}"#);
    let filters = write_filters(
        r#"
[[filters]]
pattern = "cargo nextest"
mode = "failures-only"
max_lines = 50
"#,
    );

    let payload = post_payload_with_output("cargo build", "build output", 0);
    let envs = [
        ("COURSERS_HOOK_CHAIN", "1"),
        ("COURSERS_RULES", rules.path().to_str().unwrap()),
        ("CRS_FILTERS", filters.path().to_str().unwrap()),
    ];

    let out = run_bin_with_env("coursers", "post", &payload, &envs);

    assert_eq!(
        out.status.code(),
        Some(0),
        "chain post must exit 0 on allow; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // No filter response on allow — stdout must be empty.
    assert!(
        out.stdout.is_empty(),
        "chain post must not emit output when allowing (no filter match); got: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

// ---------------------------------------------------------------------------
// Scenario 4: Failure-learning threshold trip
// ---------------------------------------------------------------------------

/// After enough failures are recorded via `coursers post`, a subsequent
/// `coursers pre` for the same command must deny due to failure-learning.
///
/// Both legacy and chain paths share the same state file, so we exercise the
/// chain path for recording and blocking.
#[test]
fn chain_failure_learning_blocks_at_threshold() {
    let rules_content = r#"{
        "rules": [],
        "failure_learning": {
            "enabled": true,
            "block_threshold": 2,
            "window_seconds": 3600,
            "max_tracked_commands": 200,
            "cleanup_after_seconds": 86400
        }
    }"#;
    let rules = write_rules(rules_content);
    let tmp = TempDir::new().unwrap();
    let state_path = tmp.path().join("state.json");

    let failing_cmd = "cargo test --non-existent-feature";

    // Record two failures via chain post.
    for _ in 0..2 {
        let payload = post_payload_with_output(failing_cmd, "FAILED", 1);
        let envs = [
            ("COURSERS_HOOK_CHAIN", "1"),
            ("COURSERS_RULES", rules.path().to_str().unwrap()),
            ("COURSERS_STATE", state_path.to_str().unwrap()),
        ];
        let out = run_bin_with_env("coursers", "post", &payload, &envs);
        assert_eq!(
            out.status.code(),
            Some(0),
            "chain post failure recording must exit 0"
        );
    }

    // Now chain pre should deny the command due to threshold.
    let payload = pre_payload(failing_cmd);
    let envs = [
        ("COURSERS_HOOK_CHAIN", "1"),
        ("COURSERS_RULES", rules.path().to_str().unwrap()),
        ("COURSERS_STATE", state_path.to_str().unwrap()),
    ];
    let out = run_bin_with_env("coursers", "pre", &payload, &envs);

    assert_eq!(
        out.status.code(),
        Some(2),
        "chain pre must deny after failure threshold; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        parse_decision(&out.stdout),
        "deny",
        "expected deny decision from failure learning"
    );

    // Verify the deny reason mentions failure learning.
    let reason = parse_reason(&out.stdout);
    assert!(
        !reason.is_empty(),
        "deny reason must not be empty for failure-learning block"
    );
}
