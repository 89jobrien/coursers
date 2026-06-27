use crs_core::history::{DiscoverOpts, discover};
use crs_core::loader::{FsRulesLoader, RulesLoader};
use crs_core::rules::Rule;
use std::path::PathBuf;

/// Stable inline rules for tests that need deterministic intercept/unhandled splits,
/// independent of the user's live ~/.config/coursers/course-correct-rules.json.
fn fixture_rules() -> Vec<Rule> {
    vec![Rule {
        id: "no-cargo-use-nextest".to_string(),
        enabled: true,
        pattern: r"^\s*cargo\b".to_string(),
        pattern_flags: String::new(),
        exceptions: vec![],
        target_commands: vec!["cargo".to_string()],
        message: None,
    }]
}

#[test]
fn jsonl_source_empty_dir_yields_no_commands() {
    let tmp = tempfile::tempdir().unwrap();
    let src = crs_lib::jsonl_source::JsonlCommandSource::new(
        tmp.path().to_path_buf(),
        false,
        std::env::current_dir().ok(),
    );
    let rules = FsRulesLoader.load().unwrap_or_default();
    let report = discover(
        &src,
        &rules.rules,
        &DiscoverOpts {
            all_projects: true,
            ..Default::default()
        },
    );
    assert_eq!(report.scanned_commands, 0);
}

/// Regression: discover used load_rules() (live user config) so adding no-bash-use-nu
/// (pattern `.`) caused ALL commands to be "intercepted", leaving unhandled empty.
/// Fix: inject fixture_rules() — a stable, deterministic rule set.
#[test]
fn regression_discover_unhandled_empty_with_catch_all_rule() {
    // no-bash-use-nu has pattern "." — matches every command not starting with "nu".
    let catch_all = vec![Rule {
        id: "no-bash-use-nu".to_string(),
        enabled: true,
        pattern: ".".to_string(),
        pattern_flags: String::new(),
        exceptions: vec![r"^nu\b".to_string()],
        target_commands: vec![],
        message: None,
    }];
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/discover");
    let src = crs_lib::jsonl_source::JsonlCommandSource::new(fixtures, true, None);
    let report = discover(
        &src,
        &catch_all,
        &crs_core::history::DiscoverOpts {
            all_projects: true,
            since_days: None,
            ..Default::default()
        },
    );
    // With a catch-all rule, all commands are intercepted — unhandled must be empty.
    assert_eq!(report.scanned_commands, 3);
    assert!(
        report.unhandled.is_empty(),
        "catch-all rule must leave unhandled empty, got: {:?}",
        report.unhandled
    );
}

#[test]
fn jsonl_source_reads_fixture_commands() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/discover");

    let src = crs_lib::jsonl_source::JsonlCommandSource::new(fixtures, true, None);

    let rules = fixture_rules();
    let report = discover(
        &src,
        &rules,
        &crs_core::history::DiscoverOpts {
            all_projects: true,
            since_days: None,
            ..Default::default()
        },
    );

    assert_eq!(report.scanned_commands, 3);
    assert_eq!(report.scanned_sessions, 1);
    assert_eq!(report.unhandled[0].stem, "doob todo");
    assert_eq!(report.unhandled[0].count, 2);
}
