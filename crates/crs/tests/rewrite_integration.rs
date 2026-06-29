//! Integration tests for `crs rewrite` — PreToolUse hook wiring.
//!
//! Tests the library function `run_rewrite` with real TOML-loaded config,
//! verifying command rewriting semantics.

use coursers_core::rewrite::{RewriteConfig, RewriteRule};
use crs_lib::run_rewrite;

fn cfg(rules: &[(&str, &str)]) -> RewriteConfig {
    RewriteConfig {
        rewrites: rules
            .iter()
            .map(|(p, r)| RewriteRule {
                pattern: p.to_string(),
                replace: r.to_string(),
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// matching rule rewrites command
// ---------------------------------------------------------------------------

#[test]
fn rewrite_matching_rule_returns_rewritten() {
    let config = cfg(&[("^cargo test(.*)", "cargo nextest run$1")]);
    let result = run_rewrite("cargo test --release", &config);
    assert_eq!(result.unwrap(), "cargo nextest run --release");
}

#[test]
fn rewrite_full_replacement() {
    let config = cfg(&[("^git status$", "git status --short")]);
    let result = run_rewrite("git status", &config);
    assert_eq!(result.unwrap(), "git status --short");
}

// ---------------------------------------------------------------------------
// no matching rule returns None (passthrough)
// ---------------------------------------------------------------------------

#[test]
fn rewrite_no_match_returns_none() {
    let config = cfg(&[("^cargo nextest", "cargo nextest run --no-fail-fast")]);
    assert!(run_rewrite("doob todo list", &config).is_none());
}

// ---------------------------------------------------------------------------
// capture groups work
// ---------------------------------------------------------------------------

#[test]
fn rewrite_capture_groups() {
    let config = cfg(&[("^(cargo build)(.*)", "$1 --color always$2")]);
    let result = run_rewrite("cargo build --release", &config);
    assert_eq!(result.unwrap(), "cargo build --color always --release");
}

// ---------------------------------------------------------------------------
// first matching rule wins
// ---------------------------------------------------------------------------

#[test]
fn rewrite_first_match_wins() {
    let config = cfg(&[
        ("^cargo nextest(.*)", "cargo nextest run --no-fail-fast$1"),
        ("^cargo(.*)", "cargo --color always$1"),
    ]);
    let result = run_rewrite("cargo nextest run", &config);
    assert_eq!(result.unwrap(), "cargo nextest run --no-fail-fast run");
}

// ---------------------------------------------------------------------------
// empty rules returns None
// ---------------------------------------------------------------------------

#[test]
fn rewrite_empty_rules_returns_none() {
    let config = RewriteConfig::default();
    assert!(run_rewrite("cargo build", &config).is_none());
}

// ---------------------------------------------------------------------------
// real TOML config loading
// ---------------------------------------------------------------------------

#[test]
fn rewrite_with_real_toml_config() {
    use std::io::Write as _;

    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
[[rewrites]]
pattern = "^cargo test(.*)"
replace = "cargo nextest run$1"

[[rewrites]]
pattern = "^git status$"
replace = "git status --short"
"#,
    )
    .unwrap();

    // Load via FiltersConfig which includes rewrites
    let content = std::fs::read_to_string(f.path()).unwrap();
    let config: RewriteConfig = toml::from_str(&content).unwrap();

    assert_eq!(
        run_rewrite("cargo test -p foo", &config).unwrap(),
        "cargo nextest run -p foo"
    );
    assert_eq!(
        run_rewrite("git status", &config).unwrap(),
        "git status --short"
    );
    assert!(run_rewrite("doob todo list", &config).is_none());
}
