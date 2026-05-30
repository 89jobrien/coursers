//! Integration tests for `crs filter` — PostToolUse hook wiring.
//!
//! Each test creates a temp TOML config, sets CRS_FILTERS, and invokes the
//! library function directly (no binary needed — the binary is just a thin
//! stdin/stdout wrapper around `run_filter`).

use crs_core::filters::{FilterMode, FilterRule, FiltersConfig};
use crs_lib::{FilterPayload, FilterResult, run_filter};

fn payload(cmd: &str, output: &str, exit_code: i64) -> FilterPayload {
    FilterPayload {
        command: cmd.to_string(),
        output: output.to_string(),
        exit_code,
    }
}

fn cfg_single(pattern: &str, mode: FilterMode, max_lines: usize) -> FiltersConfig {
    FiltersConfig {
        filters: vec![FilterRule {
            pattern: pattern.to_string(),
            mode,
            max_lines,
        }],
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// passthrough mode
// ---------------------------------------------------------------------------

#[test]
fn filter_passthrough_returns_output_unchanged() {
    let cfg = cfg_single("cargo test", FilterMode::Passthrough, 50);
    let p = payload("cargo test", "all tests passed", 0);
    assert_eq!(run_filter(&p, &cfg), FilterResult::Passthrough);
}

// ---------------------------------------------------------------------------
// failures-only mode
// ---------------------------------------------------------------------------

#[test]
fn filter_failures_only_suppresses_on_success() {
    let cfg = cfg_single("cargo test", FilterMode::FailuresOnly, 50);
    let p = payload("cargo test --release", "all good", 0);
    assert_eq!(run_filter(&p, &cfg), FilterResult::Suppress);
}

#[test]
fn filter_failures_only_passes_on_failure() {
    let cfg = cfg_single("cargo test", FilterMode::FailuresOnly, 50);
    let p = payload("cargo test --release", "FAILED", 1);
    // Non-zero exit: output passed through unchanged -> Passthrough (same text)
    assert_eq!(run_filter(&p, &cfg), FilterResult::Passthrough);
}

// ---------------------------------------------------------------------------
// errors-only mode
// ---------------------------------------------------------------------------

#[test]
fn filter_errors_only_extracts_error_lines() {
    let cfg = cfg_single("cargo build", FilterMode::ErrorsOnly, 50);
    let output =
        "Compiling foo\nerror[E0308]: mismatched types\n  --> src/main.rs:5\nwarning: unused\n";
    let p = payload("cargo build", output, 1);
    match run_filter(&p, &cfg) {
        FilterResult::Replace(text) => {
            assert!(text.contains("error[E0308]"));
            assert!(!text.contains("Compiling"));
            assert!(!text.contains("warning"));
        }
        other => panic!("expected Replace, got {other:?}"),
    }
}

#[test]
fn filter_errors_only_suppresses_when_no_errors() {
    let cfg = cfg_single("cargo build", FilterMode::ErrorsOnly, 50);
    let p = payload("cargo build", "Compiling foo\nFinished", 0);
    assert_eq!(run_filter(&p, &cfg), FilterResult::Suppress);
}

// ---------------------------------------------------------------------------
// truncate mode
// ---------------------------------------------------------------------------

#[test]
fn filter_truncate_keeps_first_n_lines() {
    let cfg = cfg_single("doob todo", FilterMode::Truncate, 3);
    let output = "line1\nline2\nline3\nline4\nline5";
    let p = payload("doob todo list", output, 0);
    match run_filter(&p, &cfg) {
        FilterResult::Replace(text) => {
            assert!(text.starts_with("line1\nline2\nline3"));
            assert!(text.contains("2 lines omitted"));
        }
        other => panic!("expected Replace, got {other:?}"),
    }
}

#[test]
fn filter_truncate_passthrough_when_under_limit() {
    let cfg = cfg_single("doob todo", FilterMode::Truncate, 10);
    let p = payload("doob todo list", "line1\nline2", 0);
    assert_eq!(run_filter(&p, &cfg), FilterResult::Passthrough);
}

// ---------------------------------------------------------------------------
// no matching rule
// ---------------------------------------------------------------------------

#[test]
fn filter_no_matching_rule_passthrough() {
    let cfg = cfg_single("cargo nextest", FilterMode::FailuresOnly, 50);
    let p = payload("doob todo list", "some output", 0);
    assert_eq!(run_filter(&p, &cfg), FilterResult::Passthrough);
}

// ---------------------------------------------------------------------------
// real TOML config loading
// ---------------------------------------------------------------------------

#[test]
fn filter_with_real_toml_config() {
    use std::io::Write as _;

    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
[[filters]]
pattern = "cargo (nextest|test)"
mode = "failures-only"

[[filters]]
pattern = "doob"
mode = "truncate"
max_lines = 5
"#,
    )
    .unwrap();

    let cfg = crs_core::filters::FiltersConfig::load_from(f.path());

    // cargo test with exit 0 -> suppressed
    let p1 = payload("cargo test --release", "output", 0);
    assert_eq!(run_filter(&p1, &cfg), FilterResult::Suppress);

    // doob with 10 lines -> truncated
    let long_output = (1..=10)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let p2 = payload("doob todo list", &long_output, 0);
    match run_filter(&p2, &cfg) {
        FilterResult::Replace(text) => {
            assert!(text.contains("5 lines omitted"));
        }
        other => panic!("expected Replace for doob, got {other:?}"),
    }
}
