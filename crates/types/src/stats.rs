use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Cumulative block statistics: counts and timestamps per rule id.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Stats {
    pub blocks: HashMap<String, u64>,
    pub last_seen: HashMap<String, f64>,
}
