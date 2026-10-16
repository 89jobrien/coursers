//! Cumulative block-count and last-seen statistics.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Cumulative block statistics: counts and timestamps per rule id.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Stats {
    pub blocks: HashMap<String, u64>,
    pub last_seen: HashMap<String, f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_default_is_empty() {
        let stats = Stats::default();
        assert!(stats.blocks.is_empty());
        assert!(stats.last_seen.is_empty());
    }

    #[test]
    fn stats_roundtrips_through_json() {
        let mut stats = Stats::default();
        stats.blocks.insert("no-find".into(), 4);
        stats.last_seen.insert("no-find".into(), 123.5);
        let json = serde_json::to_string(&stats).unwrap();
        let back: Stats = serde_json::from_str(&json).unwrap();
        assert_eq!(back.blocks["no-find"], 4);
        assert_eq!(back.last_seen["no-find"], 123.5);
    }

    #[test]
    fn stats_deserializes_missing_required_fields_errors() {
        // blocks/last_seen have no #[serde(default)], so an empty object
        // should fail to deserialize -- this pins the current (strict)
        // contract so a future accidental #[serde(default)] addition is
        // caught by a test change, not silently.
        let res: Result<Stats, _> = serde_json::from_str("{}");
        assert!(res.is_err());
    }
}
