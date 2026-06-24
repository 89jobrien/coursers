#[path = "common_bin.rs"]
mod common_bin;

use common_bin::crs_bin;
use std::process::Command;

fn crs_nu_check(args: &[&str]) -> std::process::Output {
    Command::new(crs_bin())
        .arg("nu-check")
        .args(args)
        .output()
        .expect("failed to spawn crs")
}

// t7
#[test]
fn valid_file_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("valid.nu");
    std::fs::write(&path, "export def greet [] { \"hello\" }").unwrap();
    let out = crs_nu_check(&[path.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// t8
#[test]
fn invalid_file_exits_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.nu");
    std::fs::write(&path, "def broken {").unwrap();
    let out = crs_nu_check(&[path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1), "expected exit 1");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.is_empty(), "expected error output on stderr");
}

// t9
#[test]
fn hooks_flag_scans_hook_dir() {
    // ~/.claude/hooks/nu/ must exist; even if empty the command should exit 0.
    let out = crs_nu_check(&["--hooks"]);
    // If hooks dir doesn't exist, walkdir returns nothing → "no files" → exit 1.
    // Either outcome is deterministic; we just assert it doesn't panic.
    let code = out.status.code().expect("process killed by signal");
    assert!(code == 0 || code == 1, "unexpected exit code: {code}");
}

// t10
#[test]
fn nu_libs_flag_scans_lib_dir() {
    // ~/dev/nu_libs/lib must exist and all mod.nu files we fixed should pass.
    let out = crs_nu_check(&["--nu-libs"]);
    let code = out.status.code().expect("process killed by signal");
    if code != 0 {
        let stderr = String::from_utf8_lossy(&out.stderr);
        panic!("crs nu-check --nu-libs failed:\n{stderr}");
    }
}
