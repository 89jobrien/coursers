#[path = "common_bin.rs"]
mod common_bin;

use common_bin::crs_bin;
use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

#[test]
fn validate_binary_valid_rules_exits_zero() {
    let mut f = NamedTempFile::new().unwrap();
    write!(f, r#"{{"rules":[]}}"#).unwrap();
    let out = Command::new(crs_bin())
        .arg("validate")
        .env("COURSERS_RULES", f.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "expected exit 0 with valid rules, got: {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn validate_binary_produces_health_check_output() {
    // validate emits a structured health report — verify the header is present.
    // With an empty rules file, all known-rule checks report warnings (not found),
    // but the command still exits 0 (it reports, not enforces).
    let mut f = NamedTempFile::new().unwrap();
    write!(f, r#"{{"rules":[]}}"#).unwrap();
    let out = Command::new(crs_bin())
        .arg("validate")
        .env("COURSERS_RULES", f.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "exit: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Validate") || stdout.contains("Rule") || stdout.contains("OK"),
        "expected health-check output, got: {stdout}"
    );
}
