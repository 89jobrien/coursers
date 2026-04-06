use crs_core::history::{DiscoverOpts, discover};
use crs_core::rules::load as load_rules;
use std::path::PathBuf;

#[test]
fn jsonl_source_empty_dir_yields_no_commands() {
    let tmp = tempfile::tempdir().unwrap();
    let src = crs_lib::jsonl_source::JsonlCommandSource::new(
        tmp.path().to_path_buf(),
        false,
        std::env::current_dir().ok(),
    );
    let rules = load_rules();
    let report = discover(&src, &rules.rules, &DiscoverOpts {
        all_projects: true,
        ..Default::default()
    });
    assert_eq!(report.scanned_commands, 0);
}

#[test]
fn jsonl_source_reads_fixture_commands() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/discover");

    let src = crs_lib::jsonl_source::JsonlCommandSource::new(
        fixtures,
        true,
        None,
    );

    let rules = load_rules();
    let report = discover(&src, &rules.rules, &crs_core::history::DiscoverOpts {
        all_projects: true,
        since_days: None,
        ..Default::default()
    });

    assert_eq!(report.scanned_commands, 3);
    assert_eq!(report.scanned_sessions, 1);
    assert_eq!(report.unhandled[0].stem, "doob todo");
    assert_eq!(report.unhandled[0].count, 2);
}
