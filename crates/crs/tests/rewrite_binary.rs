#[path = "common_bin.rs"]
mod common_bin;

use common_bin::run_crs;
use std::io::Write;
use tempfile::NamedTempFile;

fn rewrite_toml(pattern: &str, replace: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    write!(
        f,
        "[[rewrites]]\npattern = {pattern:?}\nreplace = {replace:?}\n"
    )
    .unwrap();
    f
}

fn pre_payload(cmd: &str) -> String {
    format!(r#"{{"tool_name":"Bash","tool_input":{{"command":{cmd:?}}}}}"#)
}

#[test]
fn rewrite_binary_matching_rule_exits_zero_with_json() {
    let cfg = rewrite_toml("^cargo test(.*)", "cargo nextest run$1");
    let payload = pre_payload("cargo test --release");
    let out = run_crs(
        "rewrite",
        &payload,
        &[("CRS_FILTERS", cfg.path().to_str().unwrap())],
    );
    assert!(
        out.status.success(),
        "exit: {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("cargo nextest run --release"),
        "expected rewritten command in output, got: {stdout}"
    );
    assert!(
        stdout.contains("updatedInput"),
        "expected hookSpecificOutput JSON, got: {stdout}"
    );
}

#[test]
fn rewrite_binary_no_match_exits_one() {
    let cfg = rewrite_toml("^cargo test(.*)", "cargo nextest run$1");
    let payload = pre_payload("doob todo list");
    let out = run_crs(
        "rewrite",
        &payload,
        &[("CRS_FILTERS", cfg.path().to_str().unwrap())],
    );
    assert!(
        !out.status.success(),
        "expected exit 1 on no match, got: {:?}",
        out.status
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.is_empty(),
        "expected empty stdout on no match, got: {stdout}"
    );
}

#[test]
fn rewrite_binary_non_bash_exits_one() {
    let cfg = rewrite_toml("^cargo test(.*)", "cargo nextest run$1");
    let payload = r#"{"tool_name":"Read","tool_input":{"command":"cargo test"}}"#;
    let out = run_crs(
        "rewrite",
        payload,
        &[("CRS_FILTERS", cfg.path().to_str().unwrap())],
    );
    assert!(
        !out.status.success(),
        "expected exit 1 for non-Bash tool, got: {:?}",
        out.status
    );
}
