//! Rule suggestion from unhandled command history.
//!
//! Given a list of unhandled command stems (from `history::discover`),
//! generates candidate rule JSON that can be pasted into the rules config.

use crate::history::CommandFreq;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SuggestedRule {
    pub id: String,
    pub pattern: String,
    pub message: String,
    pub example: String,
    pub count: u64,
}

/// Generate rule suggestions from a list of unhandled command frequencies.
///
/// Each suggestion targets the stem of the command (e.g. `grep`, `cargo test`).
/// Rules are sorted by count descending (highest-frequency first).
pub fn suggest(unhandled: &[CommandFreq]) -> Vec<SuggestedRule> {
    let mut rules: Vec<SuggestedRule> = unhandled
        .iter()
        .filter(|f| !f.stem.is_empty())
        .map(|f| {
            let (id, pattern, message) = rule_for_stem(&f.stem);
            SuggestedRule {
                id,
                pattern,
                message,
                example: f.example.clone(),
                count: f.count,
            }
        })
        .collect();

    rules.sort_by(|a, b| b.count.cmp(&a.count));
    rules
}

/// Derive a rule id, regex pattern, and message from a command stem.
fn rule_for_stem(stem: &str) -> (String, String, String) {
    // Normalise to a valid rule id: lowercase, spaces → hyphens
    let id = format!("no-{}", stem.to_lowercase().replace([' ', '/'], "-"));

    // Pattern: match the exact stem at the start of the command (after optional env vars)
    let first_token = stem.split_whitespace().next().unwrap_or(stem);
    let pattern = if stem.contains(' ') {
        // Two-token stem like "cargo test" — match both
        let tokens: Vec<&str> = stem.splitn(2, ' ').collect();
        format!(
            r"(?:^|\s){}(?:\s+{}\b)",
            regex_escape(tokens[0]),
            regex_escape(tokens[1])
        )
    } else {
        format!(r"\b{}\b", regex_escape(first_token))
    };

    let message = format!("Use the dedicated tool instead of `{stem}`.");

    (id, pattern, message)
}

/// Minimal regex escaping for literal string use in patterns.
fn regex_escape(s: &str) -> String {
    s.chars()
        .flat_map(|c| {
            if r"\.+*?()|[]{}^$#&-~".contains(c) {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect()
}

// Note: regex_escape uses char iteration which exceeds Kani's unwind budget.
// Property tests in tests/property_tests.rs and unit tests cover regex_escape.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::CommandFreq;

    fn freq(stem: &str, count: u64, example: &str) -> CommandFreq {
        CommandFreq {
            stem: stem.to_string(),
            count,
            example: example.to_string(),
            est_tokens: 0,
            rule_id: None,
        }
    }

    #[test]
    fn suggest_empty_input_returns_empty() {
        assert!(suggest(&[]).is_empty());
    }

    #[test]
    fn suggest_single_command_produces_rule() {
        let rules = suggest(&[freq("grep", 5, "grep foo .")]);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "no-grep");
        assert!(rules[0].pattern.contains("grep"));
        assert_eq!(rules[0].count, 5);
        assert_eq!(rules[0].example, "grep foo .");
    }

    #[test]
    fn suggest_sorted_by_count_descending() {
        let input = vec![
            freq("ls", 2, "ls -la"),
            freq("grep", 10, "grep foo ."),
            freq("cat", 5, "cat file.txt"),
        ];
        let rules = suggest(&input);
        assert_eq!(rules[0].id, "no-grep");
        assert_eq!(rules[1].id, "no-cat");
        assert_eq!(rules[2].id, "no-ls");
    }

    #[test]
    fn suggest_two_token_stem_makes_compound_pattern() {
        let rules = suggest(&[freq("cargo test", 3, "cargo test -p foo")]);
        assert_eq!(rules[0].id, "no-cargo-test");
        assert!(rules[0].pattern.contains("cargo"));
        assert!(rules[0].pattern.contains("test"));
    }

    #[test]
    fn suggest_skips_empty_stems() {
        let rules = suggest(&[freq("", 10, "")]);
        assert!(rules.is_empty());
    }

    #[test]
    fn suggest_message_contains_stem() {
        let rules = suggest(&[freq("grep", 1, "grep x .")]);
        assert!(rules[0].message.contains("grep"));
    }
}
