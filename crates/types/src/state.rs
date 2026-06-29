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
