use serde::{Deserialize, Serialize};

/// A suggestion record pairing a blocked command with its alternative.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuggestionRecord {
    pub ts: String,
    pub original: String,
    pub suggestion: String,
    pub rule_id: String,
    pub cwd: String,
    pub repo: Option<String>,
    pub session_id: Option<String>,
    pub tool_name: String,
    pub accepted: bool,
    pub accepted_ts: Option<String>,
    pub exit_code: Option<i64>,
    pub count: u32,
}

/// Parameters for constructing a [`SuggestionRecord`].
pub struct SuggestionParams {
    pub original: String,
    pub suggestion: String,
    pub rule_id: String,
    pub cwd: String,
    pub session_id: Option<String>,
    pub tool_name: String,
}

/// Dedup key: normalized (trimmed) original + suggestion.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DedupeKey {
    pub original: String,
    pub suggestion: String,
}

impl DedupeKey {
    pub fn from_record(r: &SuggestionRecord) -> Self {
        Self {
            original: r.original.trim().to_string(),
            suggestion: r.suggestion.trim().to_string(),
        }
    }

    pub fn from_parts(original: &str, suggestion: &str) -> Self {
        Self {
            original: original.trim().to_string(),
            suggestion: suggestion.trim().to_string(),
        }
    }
}
