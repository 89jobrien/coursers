//! Rule suggestion from unhandled command history.
//!
//! Given a list of unhandled command stems (from `history::discover`),
//! generates candidate rule JSON that can be pasted into the rules config.

use crate::history::CommandFreq;
use serde::Serialize;

/// A candidate rule auto-generated from an unhandled command stem.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SuggestedRule {
    pub id: String,
    pub pattern: String,
    pub target_commands: Vec<String>,
    pub message: String,
    pub example: String,
    pub count: u64,
    pub est_tokens: u64,
}

/// Generate rule suggestions from a list of unhandled command frequencies.
///
/// Each suggestion targets the stem of the command (e.g. `grep`, `cargo test`).
/// Rules are sorted by score descending (frequency × token estimate).
pub fn suggest(unhandled: &[CommandFreq]) -> Vec<SuggestedRule> {
    let mut rules: Vec<SuggestedRule> = unhandled
        .iter()
        .filter(|f| !f.stem.is_empty())
        .map(|f| {
            let (id, pattern, target_commands, message) = rule_for_stem(&f.stem);
            SuggestedRule {
                id,
                pattern,
                target_commands,
                message,
                example: f.example.clone(),
                count: f.count,
                est_tokens: f.est_tokens,
            }
        })
        .collect();

    // Score = count × est_tokens; higher score = more impactful rule
    rules.sort_by(|a, b| {
        let score_a = a.count * a.est_tokens.max(1);
        let score_b = b.count * b.est_tokens.max(1);
        score_b.cmp(&score_a)
    });
    rules
}

/// Emit a suggested rule as JSON ready to paste into the rules config file.
pub fn to_config_json(rule: &SuggestedRule) -> serde_json::Value {
    serde_json::json!({
        "id": rule.id,
        "enabled": false,
        "description": format!("Auto-suggested from {} session occurrences", rule.count),
        "pattern": rule.pattern,
        "pattern_flags": "",
        "target_commands": rule.target_commands,
        "exceptions": [],
        "exception_policy": "allow_if_any_match",
        "message": rule.message
    })
}

/// Known tool mappings: command stem → (target_commands, suggested tool/message).
fn known_tool_mapping(stem: &str) -> Option<(Vec<String>, String)> {
    let first = stem.split_whitespace().next().unwrap_or(stem);
    match first {
        "grep" | "rg" => Some((
            vec!["grep".into(), "rg".into()],
            "Use the Grep tool instead.".into(),
        )),
        "cat" => Some((vec!["cat".into()], "Use the Read tool instead.".into())),
        "head" | "tail" => Some((
            vec!["head".into(), "tail".into()],
            "Use the Read tool with offset/limit instead.".into(),
        )),
        "find" => Some((vec!["find".into()], "Use the Glob tool instead.".into())),
        "ls" => Some((
            vec!["ls".into()],
            "Use the Glob tool for file discovery.".into(),
        )),
        "sed" => Some((
            vec!["sed".into()],
            "Use the Edit tool for file modifications.".into(),
        )),
        "sleep" => Some((
            vec!["sleep".into()],
            "Do not sleep. Use run_in_background or find other work.".into(),
        )),
        "cd" => Some((
            vec!["cd".into()],
            "Use absolute paths or git -C instead of changing directory.".into(),
        )),
        "npm" | "npx" => Some((
            vec!["npm".into(), "npx".into()],
            "Use bun/bunx instead.".into(),
        )),
        "nvm" => Some((
            vec!["nvm".into()],
            "Use mise use node@<version> instead.".into(),
        )),
        "pip" | "pip3" => Some((
            vec!["pip".into(), "pip3".into()],
            "Use uv add or uv pip install instead.".into(),
        )),
        "awk" => Some((
            vec!["awk".into()],
            "Use the Grep tool or Edit tool instead of awk.".into(),
        )),
        "curl" | "wget" => Some((
            vec!["curl".into(), "wget".into()],
            "Use the WebFetch tool for HTTP requests.".into(),
        )),
        _ => None,
    }
}

/// Derive a rule id, regex pattern, target_commands, and message from a command stem.
fn rule_for_stem(stem: &str) -> (String, String, Vec<String>, String) {
    let id = format!("no-{}", stem.to_lowercase().replace([' ', '/'], "-"));
    let first_token = stem.split_whitespace().next().unwrap_or(stem);

    let pattern = if stem.contains(' ') {
        let tokens: Vec<&str> = stem.splitn(2, ' ').collect();
        format!(
            r"(?:^|\s){}(?:\s+{}\b)",
            regex_escape(tokens[0]),
            regex_escape(tokens[1])
        )
    } else {
        format!(r"\b{}\b", regex_escape(first_token))
    };

    if let Some((targets, message)) = known_tool_mapping(stem) {
        return (id, pattern, targets, message);
    }

    let target_commands = vec![first_token.to_string()];
    let message = format!("Consider using a dedicated tool instead of `{stem}`.");

    (id, pattern, target_commands, message)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::CommandFreq;

    fn freq(stem: &str, count: u64, example: &str) -> CommandFreq {
        CommandFreq {
            stem: stem.to_string(),
            count,
            example: example.to_string(),
            est_tokens: count * 10, // 10 tokens per occurrence for testing
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
    fn suggest_sorted_by_score_descending() {
        let input = vec![
            freq("ls", 2, "ls -la"),
            freq("grep", 10, "grep foo ."),
            freq("cat", 5, "cat file.txt"),
        ];
        let rules = suggest(&input);
        // grep: 10 * 100 = 1000, cat: 5 * 50 = 250, ls: 2 * 20 = 40
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
        assert!(rules[0].message.contains("Grep"));
    }

    // ── target_commands tests ────────────────────────────────────────────

    #[test]
    fn known_commands_get_target_commands() {
        let rules = suggest(&[freq("grep", 5, "grep foo .")]);
        assert_eq!(rules[0].target_commands, vec!["grep", "rg"]);
    }

    #[test]
    fn unknown_commands_get_stem_as_target() {
        let rules = suggest(&[freq("rustup", 3, "rustup show")]);
        assert_eq!(rules[0].target_commands, vec!["rustup"]);
    }

    #[test]
    fn to_config_json_has_all_fields() {
        let rules = suggest(&[freq("grep", 5, "grep foo .")]);
        let json = to_config_json(&rules[0]);
        assert_eq!(json["id"], "no-grep");
        assert_eq!(json["enabled"], false);
        assert!(json["target_commands"].is_array());
        assert!(json["exceptions"].is_array());
    }

    #[test]
    fn curl_maps_to_webfetch() {
        let rules = suggest(&[freq("curl", 10, "curl -s https://example.com")]);
        assert!(rules[0].message.contains("WebFetch"));
        assert_eq!(rules[0].target_commands, vec!["curl", "wget"]);
    }

    #[test]
    fn sleep_maps_to_run_in_background() {
        let rules = suggest(&[freq("sleep", 3, "sleep 5")]);
        assert!(rules[0].message.contains("run_in_background"));
    }
}
