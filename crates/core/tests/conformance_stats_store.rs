//! Conformance tests for `StatsStore` implementations.
//!
//! Both `FsStatsStore` and `InMemoryStatsStore` must satisfy the same observable
//! contract through the `StatsStore` trait.

use crs_core::stats::{FsStatsStore, InMemoryStatsStore, StatsStore};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Contract assertion
// ---------------------------------------------------------------------------

fn assert_stats_store_contract(store: &impl StatsStore) {
    // 1. Empty store loads with empty blocks and last_seen.
    let stats = store.load().unwrap_or_default();
    assert!(
        stats.blocks.is_empty(),
        "contract 1: fresh store must have empty blocks"
    );
    assert!(
        stats.last_seen.is_empty(),
        "contract 1: fresh store must have empty last_seen"
    );

    // 2. save() then load() round-trips faithfully.
    let mut stats = store.load().unwrap_or_default();
    stats.blocks.insert("rule-a".to_string(), 5);
    stats.last_seen.insert("rule-a".to_string(), 1000.0);
    store.save(&stats).expect("contract 2: save must succeed");
    let loaded = store.load().expect("contract 2: load must succeed");
    assert_eq!(
        loaded.blocks.get("rule-a").copied(),
        Some(5),
        "contract 2: block count must round-trip"
    );
    assert_eq!(
        loaded.last_seen.get("rule-a").copied(),
        Some(1000.0),
        "contract 2: last_seen must round-trip"
    );

    // 3. record_block increments exactly once per call.
    store
        .record_block("rule-b")
        .expect("contract 3: record_block must succeed");
    let after_one = store.load().unwrap();
    assert_eq!(
        after_one.blocks.get("rule-b").copied(),
        Some(1),
        "contract 3: first record_block must set count to 1"
    );

    store
        .record_block("rule-b")
        .expect("contract 3: second record_block");
    let after_two = store.load().unwrap();
    assert_eq!(
        after_two.blocks.get("rule-b").copied(),
        Some(2),
        "contract 3: second record_block must set count to 2"
    );

    // 4. record_block sets last_seen for the rule.
    assert!(
        after_two.last_seen.contains_key("rule-b"),
        "contract 4: record_block must populate last_seen"
    );
    assert!(
        *after_two.last_seen.get("rule-b").unwrap() > 0.0,
        "contract 4: last_seen must be a positive timestamp"
    );

    // 5. Multiple rules tracked independently.
    store
        .record_block("rule-c")
        .expect("contract 5: record_block rule-c");
    let multi = store.load().unwrap();
    assert_eq!(
        multi.blocks.get("rule-b").copied(),
        Some(2),
        "contract 5: rule-b count unchanged after rule-c increment"
    );
    assert_eq!(
        multi.blocks.get("rule-c").copied(),
        Some(1),
        "contract 5: rule-c tracked independently"
    );
    // rule-a was set in step 2 and must still be present
    assert_eq!(
        multi.blocks.get("rule-a").copied(),
        Some(5),
        "contract 5: rule-a preserved across record_block calls"
    );

    // 6. save() overwrites previous state (not append).
    let mut fresh = crs_core::stats::Stats::default();
    fresh.blocks.insert("rule-z".to_string(), 99);
    store.save(&fresh).expect("contract 6: overwrite save");
    let overwritten = store.load().unwrap();
    assert_eq!(
        overwritten.blocks.len(),
        1,
        "contract 6: save must overwrite, not append"
    );
    assert!(
        overwritten.blocks.contains_key("rule-z"),
        "contract 6: new key must be present"
    );
    assert!(
        !overwritten.blocks.contains_key("rule-a"),
        "contract 6: old key must be gone"
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn fs_stats_store_satisfies_contract() {
    let dir = TempDir::new().expect("tempdir");
    let store = FsStatsStore {
        path: dir.path().join("stats.json"),
    };
    assert_stats_store_contract(&store);
}

#[test]
fn in_memory_stats_store_satisfies_contract() {
    let store = InMemoryStatsStore::new();
    assert_stats_store_contract(&store);
}
