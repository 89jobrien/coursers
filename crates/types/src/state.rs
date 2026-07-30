use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A per-command failure record stored in the rolling failure log.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FailureEntry {
    pub command_preview: String,
    pub timestamps: Vec<u64>,
    pub last_seen: f64,
}

/// The full failure-learning state persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
    #[serde(default)]
    pub failures: HashMap<String, FailureEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_default_is_empty() {
        let state = State::default();
        assert!(state.failures.is_empty());
    }

    #[test]
    fn state_deserializes_missing_failures_as_empty() {
        let state: State = serde_json::from_str("{}").unwrap();
        assert!(state.failures.is_empty());
    }

    #[test]
    fn state_roundtrips_through_json() {
        let mut state = State::default();
        state.failures.insert(
            "git push --force".into(),
            FailureEntry {
                command_preview: "git push --force".into(),
                timestamps: vec![1, 2, 3],
                last_seen: 3.0,
            },
        );
        let json = serde_json::to_string(&state).unwrap();
        let back: State = serde_json::from_str(&json).unwrap();
        assert_eq!(back.failures.len(), 1);
        let entry = &back.failures["git push --force"];
        assert_eq!(entry.timestamps, vec![1, 2, 3]);
        assert_eq!(entry.last_seen, 3.0);
    }

    #[test]
    fn failure_entry_default_is_empty() {
        let entry = FailureEntry::default();
        assert_eq!(entry.command_preview, "");
        assert!(entry.timestamps.is_empty());
        assert_eq!(entry.last_seen, 0.0);
    }
}
