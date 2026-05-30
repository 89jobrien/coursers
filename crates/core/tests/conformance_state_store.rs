//! Conformance tests for `StateStore` implementations.
//!
//! Both `FsStateStore` and `InMemoryStateStore` must satisfy the same observable
//! contract through the `StateStore` trait.

use crs_core::state::FailureEntry;
use crs_core::store::{FsStateStore, InMemoryStateStore, StateStore};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Contract assertion
// ---------------------------------------------------------------------------

fn assert_state_store_contract(store: &impl StateStore) {
    // 1. load() on empty store returns State with empty failures HashMap
    let state = store.load();
    assert!(
        state.failures.is_empty(),
        "contract 1: fresh store must have empty failures"
    );

    // 2. save() then load() round-trips
    let mut state = store.load();
    state.failures.insert(
        "key-a".to_string(),
        FailureEntry {
            command_preview: "echo hello".to_string(),
            timestamps: vec![100, 200],
            last_seen: 200.0,
        },
    );
    store.save(&state);
    let loaded = store.load();
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
    assert_eq!(entry.timestamps, vec![100, 200]);
    assert_eq!(entry.last_seen, 200.0);

    // 3. save() overwrites previous state (not append)
    let mut state2 = store.load();
    state2.failures.clear();
    state2.failures.insert(
        "key-b".to_string(),
        FailureEntry {
            command_preview: "cargo build".to_string(),
            timestamps: vec![300],
            last_seen: 300.0,
        },
    );
    store.save(&state2);
    let loaded2 = store.load();
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

    // 4. path() returns a valid path
    let path = store.path();
    assert!(
        !path.as_os_str().is_empty(),
        "contract 4: path must not be empty"
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
