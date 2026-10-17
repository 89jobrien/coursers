//! Table-driven conformance test: one case-group per course-correct rule.
//!
//! Loads a vendored snapshot of `course-correct-rules.json` (not the live
//! `~/.config/coursers/...` file — tests must be reproducible in CI without a
//! user homedir) and, for every enabled rule, replays fixture commands from
//! `tests/fixtures/rule_conformance/<rule-id>.{block,pass}.txt`.
//!
//! `.block.txt` lines are expected to be denied by `check_pipeline`.
//! `.pass.txt` lines are expected to pass through (false positives the rule
//! must not trip, or documented exceptions).
//!
//! Commands are read from files rather than embedded as Rust string literals
//! so that the guardrail hooks governing this *session* never fire against
//! the test's own source — fixture files are plain data, not Bash tool calls.

use coursers_core::rules::{Rule, RulesConfig, check_pipeline};
use std::path::Path;

const SNAPSHOT: &str = include_str!("fixtures/rule_conformance/rules_snapshot.json");

fn load_rules() -> Vec<Rule> {
    let cfg: RulesConfig =
        serde_json::from_str(SNAPSHOT).expect("vendored rules snapshot must parse");
    cfg.rules
}

/// Read a fixture file, splitting on `---`-only lines into one test case per
/// paragraph. A case may span multiple lines (e.g. a heredoc body), so cases
/// are *not* split on newlines the way a flat line-list would be. Blank
/// paragraphs and paragraphs whose first line starts with `#` are dropped.
/// Returns `None` if the file doesn't exist (not every rule has both cases).
fn read_fixture(rule_id: &str, suffix: &str) -> Option<Vec<String>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/rule_conformance")
        .join(format!("{rule_id}.{suffix}.txt"));
    let content = std::fs::read_to_string(&path).ok()?;
    Some(
        content
            .split("\n---\n")
            .map(str::trim)
            .filter(|p| !p.is_empty() && !p.starts_with('#'))
            .map(str::to_string)
            .collect(),
    )
}

/// Full table-driven sweep: every enabled rule must have at least a `.block.txt`
/// fixture, and every line in it must be denied; every line in `.pass.txt` (if
/// present) must not be denied.
#[test]
fn every_rule_has_conformance_cases_and_they_hold() {
    let rules = load_rules();
    assert!(!rules.is_empty(), "vendored snapshot produced zero rules");

    let mut missing_fixtures = Vec::new();
    let mut failures = Vec::new();

    for rule in &rules {
        if !rule.enabled {
            continue;
        }

        let Some(blocked) = read_fixture(&rule.id, "block") else {
            missing_fixtures.push(rule.id.clone());
            continue;
        };
        assert!(
            !blocked.is_empty(),
            "{}: .block.txt exists but has no fixture lines",
            rule.id
        );

        for cmd in &blocked {
            let verdict = check_pipeline(cmd, std::slice::from_ref(rule));
            if verdict.is_none() {
                failures.push(format!("{}: expected BLOCK, got PASS for: {cmd}", rule.id));
            }
        }

        if let Some(passed) = read_fixture(&rule.id, "pass") {
            for cmd in &passed {
                if let Some((_, msg)) = check_pipeline(cmd, std::slice::from_ref(rule)) {
                    failures.push(format!(
                        "{}: expected PASS, got BLOCK for: {cmd} ({msg:?})",
                        rule.id
                    ));
                }
            }
        }
    }

    assert!(
        missing_fixtures.is_empty(),
        "rules missing .block.txt fixtures: {missing_fixtures:?}"
    );
    assert!(
        failures.is_empty(),
        "conformance failures:\n{}",
        failures.join("\n")
    );
}

/// Coverage sanity: the vendored snapshot must still have 19 rules (the count
/// this harness was built against). A drift here means the snapshot is stale
/// and the `rules_snapshot.json` fixture needs regenerating from the live
/// `~/.config/coursers/course-correct-rules.json`.
#[test]
fn snapshot_rule_count_matches_expected() {
    let rules = load_rules();
    assert_eq!(rules.len(), 19, "expected 19 rules in vendored snapshot");
}

/// Every rule id appearing in the snapshot has exactly one fixture pair on
/// disk — catches a rule added to the live config but never given fixtures.
#[test]
fn no_orphaned_or_unfixtured_rule_ids() {
    let rules = load_rules();
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rule_conformance");
    let on_disk: std::collections::HashSet<String> = std::fs::read_dir(&fixture_dir)
        .expect("fixture dir must exist")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            name.strip_suffix(".block.txt").map(str::to_string)
        })
        .collect();

    for rule in &rules {
        assert!(
            on_disk.contains(&rule.id),
            "rule '{}' has no .block.txt fixture on disk",
            rule.id
        );
    }
}
