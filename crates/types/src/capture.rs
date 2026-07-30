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

#[cfg(test)]
mod tests {
    use super::*;

    fn record(original: &str, suggestion: &str) -> SuggestionRecord {
        SuggestionRecord {
            ts: "2026-01-01T00:00:00Z".into(),
            original: original.into(),
            suggestion: suggestion.into(),
            rule_id: "rule-1".into(),
            cwd: "/tmp".into(),
            repo: None,
            session_id: None,
            tool_name: "Bash".into(),
            accepted: false,
            accepted_ts: None,
            exit_code: None,
            count: 1,
        }
    }

    #[test]
    fn dedupe_key_from_record_trims_whitespace() {
        let r = record("  cat foo  ", " bat foo ");
        let key = DedupeKey::from_record(&r);
        assert_eq!(key.original, "cat foo");
        assert_eq!(key.suggestion, "bat foo");
    }

    #[test]
    fn dedupe_key_from_parts_matches_from_record() {
        let r = record("grep x", "rg x");
        assert_eq!(
            DedupeKey::from_record(&r),
            DedupeKey::from_parts("grep x", "rg x")
        );
    }

    #[test]
    fn dedupe_key_equality_ignores_surrounding_whitespace_only() {
        let a = DedupeKey::from_parts("foo", "bar");
        let b = DedupeKey::from_parts("  foo  ", "  bar  ");
        assert_eq!(a, b);
    }

    #[test]
    fn dedupe_key_hash_consistent_with_eq() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(DedupeKey::from_parts("foo", "bar"));
        assert!(set.contains(&DedupeKey::from_parts(" foo ", " bar ")));
    }

    #[test]
    fn suggestion_record_serde_roundtrip() {
        let r = record("cat foo", "bat foo");
        let json = serde_json::to_string(&r).unwrap();
        let back: SuggestionRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(r, back);
    }
}
