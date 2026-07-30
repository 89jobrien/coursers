//! Integration test for `crs hook --target codex`: verifies the deny exit code
//! from a Codex crux backend is propagated back through the crs process.
#[path = "common_bin.rs"]
mod common_bin;

use common_bin::crs_bin;
use std::io::Write;
use std::process::{Command, Stdio};

/// Writes a fake `crux` executable to `dir` that ignores its arguments and
/// exits with `code`, optionally writing `stderr_msg` to stderr first.
fn write_fake_crux(dir: &std::path::Path, code: i32, stderr_msg: &str) {
    let script_path = dir.join("crux");
    let script = format!("#!/bin/sh\nprintf '%s' \"{stderr_msg}\" 1>&2\nexit {code}\n");
    std::fs::write(&script_path, script).unwrap();
    let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&script_path, perms).unwrap();
}

fn fake_path_env(fake_bin_dir: &std::path::Path) -> String {
    let real_path = std::env::var("PATH").unwrap_or_default();
    format!("{}:{real_path}", fake_bin_dir.display())
}

#[test]
fn codex_hook_propagates_backend_deny_exit_code() {
    let tmp = tempfile::tempdir().unwrap();
    write_fake_crux(tmp.path(), 2, "denied: blocked command");

    let mut child = Command::new(crs_bin())
        .args(["hook", "--target", "codex", "pre-tool-use"])
        .env("HOME", tmp.path())
        .env("PATH", fake_path_env(tmp.path()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn crs hook --target codex");

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(br#"{"tool_name":"Bash","tool_input":{"command":"echo hi"}}"#)
        .unwrap();
    let out = child.wait_with_output().unwrap();

    assert_eq!(
        out.status.code(),
        Some(2),
        "expected crs to propagate the Codex backend's deny exit code; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("denied: blocked command"),
        "expected backend stderr to be forwarded, got: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn codex_hook_exits_zero_when_backend_allows() {
    let tmp = tempfile::tempdir().unwrap();
    write_fake_crux(tmp.path(), 0, "");

    let mut child = Command::new(crs_bin())
        .args(["hook", "--target", "codex", "pre-tool-use"])
        .env("HOME", tmp.path())
        .env("PATH", fake_path_env(tmp.path()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn crs hook --target codex");

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(br#"{"tool_name":"Bash","tool_input":{"command":"echo hi"}}"#)
        .unwrap();
    let out = child.wait_with_output().unwrap();

    assert!(
        out.status.success(),
        "expected exit 0 when backend allows; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
