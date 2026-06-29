//! Conformance tests for `CommandSource` implementations.

use coursers_core::history::CommandSource;
use coursers_core::testing::MockCommandSource;

// ---------------------------------------------------------------------------
// Contract assertion
// ---------------------------------------------------------------------------

fn assert_command_source_contract(source: &impl CommandSource, expected_count: usize) {
    // 1. commands() returns an iterator over all records
    let records: Vec<_> = source.commands().collect();
    assert_eq!(
        records.len(),
        expected_count,
        "contract 1: iterator must yield expected count"
    );

    // 2. Each record preserves all fields (checked below per test)

    // 3. Re-iteration yields same count (not consumed)
    let records2: Vec<_> = source.commands().collect();
    assert_eq!(
        records.len(),
        records2.len(),
        "contract 3: re-iteration must be stable"
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn empty_source_returns_empty_iterator() {
    let source = MockCommandSource(vec![]);
    assert_command_source_contract(&source, 0);
}

#[test]
fn mock_source_with_records_satisfies_contract() {
    use coursers_core::history::CommandRecord;

    let records = vec![
        CommandRecord {
            command: "cargo build".to_string(),
            session_id: "sess-1".to_string(),
            cwd: "/project".to_string(),
            timestamp: Some("2024-01-01T00:00:00Z".to_string()),
            output_bytes: Some(1024),
        },
        CommandRecord {
            command: "cargo test".to_string(),
            session_id: "sess-2".to_string(),
            cwd: "/other".to_string(),
            timestamp: None,
            output_bytes: None,
        },
    ];
    let source = MockCommandSource(records);
    assert_command_source_contract(&source, 2);

    // Verify field preservation (contract 2)
    let recs: Vec<_> = source.commands().collect();
    assert_eq!(recs[0].command, "cargo build");
    assert_eq!(recs[0].session_id, "sess-1");
    assert_eq!(recs[0].cwd, "/project");
    assert_eq!(recs[0].timestamp.as_deref(), Some("2024-01-01T00:00:00Z"));
    assert_eq!(recs[0].output_bytes, Some(1024));

    assert_eq!(recs[1].command, "cargo test");
    assert!(recs[1].timestamp.is_none());
    assert!(recs[1].output_bytes.is_none());
}

#[test]
fn mock_source_via_mock_workspace_satisfies_contract() {
    use coursers_core::testing::MockWorkspace;

    let ws = MockWorkspace::new()
        .with_command("grep foo .")
        .with_command_ts("cargo build", "2099-01-01T00:00:00Z")
        .with_command_bytes("cargo test", 4096);

    let source = ws.command_source();
    assert_command_source_contract(&source, 3);
}
