use crs_core::history::{DiscoverOpts, discover};
use crs_core::rules::load as load_rules;

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
