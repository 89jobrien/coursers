#[path = "common_bin.rs"]
mod common_bin;

use common_bin::run_crs;
use std::io::Write;
use tempfile::NamedTempFile;

fn filter_toml(pattern: &str, mode: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    write!(f, "[[filters]]\npattern = {pattern:?}\nmode = {mode:?}\n").unwrap();
    f
}

fn post_payload(cmd: &str, output: &str, exit_code: i32) -> String {
    format!(
        r#"{{"tool_name":"Bash","tool_input":{{"command":{cmd:?}}},"tool_response":{{"output":{output:?},"exit_code":{exit_code}}}}}"#
    )
}

#[test]
fn filter_binary_suppress_on_exit_zero() {
    let cfg = filter_toml("cargo test", "failures-only");
    let payload = post_payload("cargo test --release", "all passed", 0);
    let out = run_crs(
        "filter",
        &payload,
        &[("CRS_FILTERS", cfg.path().to_str().unwrap())],
    );
    assert!(out.status.success(), "exit: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(r#""message":""#),
        "expected suppress (empty message), got: {stdout}"
    );
}

#[test]
fn filter_binary_passthrough_on_no_matching_rule() {
    let cfg = filter_toml("cargo nextest", "failures-only");
    let payload = post_payload("doob todo list", "some output", 0);
    let out = run_crs(
        "filter",
        &payload,
        &[("CRS_FILTERS", cfg.path().to_str().unwrap())],
    );
    assert!(out.status.success(), "exit: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.is_empty() || !stdout.contains(r#""message""#),
        "expected silent passthrough, got: {stdout}"
    );
}

#[test]
fn filter_binary_replace_on_truncate() {
    let mut f = NamedTempFile::new().unwrap();
    write!(
        f,
        "[[filters]]\npattern = \"doob\"\nmode = \"truncate\"\nmax_lines = 2\n"
    )
    .unwrap();
    let long = "line1\nline2\nline3\nline4\nline5";
    let payload = post_payload("doob todo list", long, 0);
    let out = run_crs(
        "filter",
        &payload,
        &[("CRS_FILTERS", f.path().to_str().unwrap())],
    );
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("omitted"),
        "expected truncation marker, got: {stdout}"
    );
}

#[test]
fn filter_binary_non_bash_tool_is_silent() {
    let cfg = filter_toml("cargo test", "failures-only");
    let payload = r#"{"tool_name":"Read","tool_input":{"command":"cargo test"},"tool_response":{"output":"ok","exit_code":0}}"#;
    let out = run_crs(
        "filter",
        payload,
        &[("CRS_FILTERS", cfg.path().to_str().unwrap())],
    );
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.is_empty() || !stdout.contains(r#""message""#),
        "expected silent for non-Bash tool, got: {stdout}"
    );
}
