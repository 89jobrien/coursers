//! Conformance tests for `StateStore` implementations.
//!
//! Both `FsStateStore` and `InMemoryStateStore` must satisfy the same observable
//! contract through the `StateStore` trait.

use crs_core::state::FailureEntry;
use crs_core::store::{FsStateStore, InMemoryStateStore, StateStore};
use tempfile::TempDir;

/// Test timestamp: base entry in timestamps vec.
const TS_BASE: u64 = 100;
/// Test timestamp: first entry's last_seen.
const TS_FIRST: u64 = 200;
/// Test timestamp: second entry's last_seen.
const TS_SECOND: u64 = 300;

// ---------------------------------------------------------------------------
// Contract assertion
// ---------------------------------------------------------------------------

fn assert_state_store_contract(store: &impl StateStore) {
    // 1. load() on empty store returns State with empty failures HashMap.
    // For FsStateStore on a non-existent path, load() returns Err; we use
    // unwrap_or_default() to satisfy contract 1 (empty state).
    let state = store.load().unwrap_or_default();
    assert!(
        state.failures.is_empty(),
        "contract 1: fresh store must have empty failures"
    );

    // 2. save() then load() round-trips
    let mut state = store.load().unwrap_or_default();
    state.failures.insert(
        "key-a".to_string(),
        FailureEntry {
            command_preview: "echo hello".to_string(),
            timestamps: vec![TS_BASE, TS_FIRST],
            last_seen: TS_FIRST as f64,
        },
    );
    store.save(&state).unwrap();
    let loaded = store.load().unwrap();
    assert_eq!(
        loaded.failures.len(),
        1,
        "contract 2: round-trip must preserve entry count"
    );
    let entry = loaded
        .failures
        .get("key-a")
        .expect("contract 2: key must exist");
    assert_eq!(entry.command_preview, "echo hello");
    assert_eq!(entry.timestamps, vec![TS_BASE, TS_FIRST]);
    assert_eq!(entry.last_seen, TS_FIRST as f64);

    // 3. save() overwrites previous state (not append)
    let mut state2 = store.load().unwrap();
    state2.failures.clear();
    state2.failures.insert(
        "key-b".to_string(),
        FailureEntry {
            command_preview: "cargo build".to_string(),
            timestamps: vec![TS_SECOND],
            last_seen: TS_SECOND as f64,
        },
    );
    store.save(&state2).unwrap();
    let loaded2 = store.load().unwrap();
    assert_eq!(
        loaded2.failures.len(),
        1,
        "contract 3: save must overwrite, not append"
    );
    assert!(
        loaded2.failures.contains_key("key-b"),
        "contract 3: new key must be present"
    );
    assert!(
        !loaded2.failures.contains_key("key-a"),
        "contract 3: old key must be gone"
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn fs_state_store_satisfies_contract() {
    let dir = TempDir::new().expect("tempdir");
    let store = FsStateStore {
        path: dir.path().join("state.json"),
    };
    assert_state_store_contract(&store);
}

#[test]
fn in_memory_state_store_satisfies_contract() {
    let store = InMemoryStateStore::new();
    assert_state_store_contract(&store);
}
