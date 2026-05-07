//! Conformance tests for `CaptureStore` implementations.
//!
//! Every `impl CaptureStore` must satisfy the observable-through-trait contracts
//! checked by `assert_mark_accepted_contract`. Dedup behaviour (contract 2) is
//! observable only on `SuggestionStore` because `InMemoryCaptureStore` exposes
//! its backing `Vec` directly; that contract is tested via `SuggestionStore`'s
//! own `load()` helper in the separate block below.

use crs_core::capture::{CaptureStore, InMemoryCaptureStore, SuggestionRecord, SuggestionStore};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Shared fixture helpers
// ---------------------------------------------------------------------------

fn make_record(original: &str, suggestion: &str, session: &str) -> SuggestionRecord {
    SuggestionRecord::new(
        original,
        suggestion,
        "no-grep-use-tool",
        "/tmp/conformance",
        Some(session.to_string()),
        "Bash",
    )
}

// ---------------------------------------------------------------------------
// Contract: mark_accepted semantics (observable through trait alone)
//
// 3. mark_accepted() with matching session_id + suggestion sets accepted=true
// 4. mark_accepted() with wrong session_id does not mutate
// 5. mark_accepted() with wrong command does not mutate
// 6. once accepted=true, a second mark_accepted() is idempotent (stays true)
// ---------------------------------------------------------------------------

fn assert_mark_accepted_contract(store: &InMemoryCaptureStore) {
    // -----------------------------------------------------------------------
    // 3. Correct session + suggestion → accepted
    // -----------------------------------------------------------------------
    store.record(make_record("grep a .", "rg a .", "sess-A"));
    store.mark_accepted("sess-A", "rg a .", 0);
    let recs = store.records();
    assert_eq!(recs.len(), 1, "contract 3: expected 1 record");
    assert!(recs[0].accepted, "contract 3: record should be accepted");
    assert_eq!(recs[0].exit_code, Some(0), "contract 3: exit_code stored");
    assert!(
        recs[0].accepted_ts.is_some(),
        "contract 3: accepted_ts populated"
    );

    // -----------------------------------------------------------------------
    // 4. Wrong session_id → no mutation (use a fresh unaccepted record)
    // -----------------------------------------------------------------------
    store.record(make_record("find . -name x", "fd x", "sess-B"));
    store.mark_accepted("sess-WRONG", "fd x", 0);
    let recs = store.records();
    let fd_rec = recs
        .iter()
        .find(|r| r.original == "find . -name x")
        .expect("contract 4: record must exist");
    assert!(
        !fd_rec.accepted,
        "contract 4: wrong session must not set accepted"
    );

    // -----------------------------------------------------------------------
    // 5. Wrong command → no mutation
    // -----------------------------------------------------------------------
    store.record(make_record("cat file.txt", "bat file.txt", "sess-C"));
    store.mark_accepted("sess-C", "totally-wrong-command", 0);
    let recs = store.records();
    let cat_rec = recs
        .iter()
        .find(|r| r.original == "cat file.txt")
        .expect("contract 5: record must exist");
    assert!(
        !cat_rec.accepted,
        "contract 5: wrong command must not set accepted"
    );

    // -----------------------------------------------------------------------
    // 6. Already accepted → second mark_accepted stays true (idempotent)
    // -----------------------------------------------------------------------
    // Record from contract 3 is already accepted. Call mark_accepted again.
    store.mark_accepted("sess-A", "rg a .", 99);
    let recs = store.records();
    let rg_rec = recs
        .iter()
        .find(|r| r.original == "grep a .")
        .expect("contract 6: record must exist");
    assert!(
        rg_rec.accepted,
        "contract 6: accepted must remain true after second call"
    );
    // exit_code should not have been overwritten to 99 (already accepted, skipped)
    assert_eq!(
        rg_rec.exit_code,
        Some(0),
        "contract 6: exit_code not overwritten on already-accepted record"
    );
}

// ---------------------------------------------------------------------------
// SuggestionStore: full 6-point contract including dedup (contract 1+2)
// ---------------------------------------------------------------------------

fn assert_fs_store_contract(store: &SuggestionStore) {
    // -----------------------------------------------------------------------
    // 1. record() with new (original, suggestion) pair is stored
    // -----------------------------------------------------------------------
    store.record(make_record("grep a .", "rg a .", "sess-A"));
    let recs = store.load();
    assert_eq!(recs.len(), 1, "contract 1: new pair stored");
    assert_eq!(recs[0].count, 1, "contract 1: count starts at 1");

    // -----------------------------------------------------------------------
    // 2. duplicate record() increments count, does not add second entry
    // -----------------------------------------------------------------------
    store.record(make_record("grep a .", "rg a .", "sess-A"));
    let recs = store.load();
    assert_eq!(recs.len(), 1, "contract 2: no duplicate entry");
    assert_eq!(recs[0].count, 2, "contract 2: count incremented");

    // -----------------------------------------------------------------------
    // 3. mark_accepted() with matching session_id + suggestion sets accepted
    // -----------------------------------------------------------------------
    store.mark_accepted("sess-A", "rg a .", 0);
    let recs = store.load();
    assert!(recs[0].accepted, "contract 3: record accepted");
    assert_eq!(recs[0].exit_code, Some(0));
    assert!(recs[0].accepted_ts.is_some());

    // -----------------------------------------------------------------------
    // 4. mark_accepted() with wrong session_id does not mutate
    // -----------------------------------------------------------------------
    store.record(make_record("find . -name x", "fd x", "sess-B"));
    store.mark_accepted("sess-WRONG", "fd x", 0);
    let recs = store.load();
    let fd_rec = recs
        .iter()
        .find(|r| r.original == "find . -name x")
        .unwrap();
    assert!(!fd_rec.accepted, "contract 4: wrong session no change");

    // -----------------------------------------------------------------------
    // 5. mark_accepted() with wrong command does not mutate
    // -----------------------------------------------------------------------
    store.record(make_record("cat file.txt", "bat file.txt", "sess-C"));
    store.mark_accepted("sess-C", "totally-wrong", 0);
    let recs = store.load();
    let cat_rec = recs.iter().find(|r| r.original == "cat file.txt").unwrap();
    assert!(!cat_rec.accepted, "contract 5: wrong command no change");

    // -----------------------------------------------------------------------
    // 6. once accepted=true, a second mark_accepted() is idempotent
    // -----------------------------------------------------------------------
    store.mark_accepted("sess-A", "rg a .", 99);
    let recs = store.load();
    let rg_rec = recs.iter().find(|r| r.original == "grep a .").unwrap();
    assert!(rg_rec.accepted, "contract 6: still accepted");
    assert_eq!(
        rg_rec.exit_code,
        Some(0),
        "contract 6: exit_code not overwritten"
    );
}

// ---------------------------------------------------------------------------
// Test functions
// ---------------------------------------------------------------------------

#[test]
fn fs_store_satisfies_contract() {
    let dir = TempDir::new().expect("tempdir");
    let store = SuggestionStore::new(dir.path().join("s.jsonl"));
    assert_fs_store_contract(&store);
}

#[test]
fn in_memory_store_satisfies_contract() {
    let store = InMemoryCaptureStore::new();
    assert_mark_accepted_contract(&store);
}
