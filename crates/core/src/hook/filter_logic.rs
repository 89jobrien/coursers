//! Parsing and evaluation for PostToolUse output filters.

use crate::filters::{FilterMode, FilterRule, FiltersConfig, find_rule};

/// Parsed PostToolUse hook payload used by the filter subcommand.
pub use coursers_types::filters::{FilterPayload, FilterResult};

/// Apply filter to output based on mode and exit code.
/// Returns the (possibly modified) output string, or None to suppress entirely.
pub fn apply_filter(output: &str, exit_code: i64, rule: &FilterRule) -> Option<String> {
    match rule.mode {
        FilterMode::Passthrough => Some(output.to_string()),
        FilterMode::FailuresOnly => {
            if exit_code != 0 {
                Some(output.to_string())
            } else {
                None
            }
        }
        FilterMode::ErrorsOnly => {
            let filtered: Vec<&str> = output
                .lines()
                .filter(|l| l.to_lowercase().contains("error"))
                .collect();
            if filtered.is_empty() {
                None
            } else {
                Some(filtered.join("\n"))
            }
        }
        FilterMode::Truncate => {
            let lines: Vec<&str> = output.lines().collect();
            if lines.len() <= rule.max_lines {
                Some(output.to_string())
            } else {
                let kept = lines[..rule.max_lines].join("\n");
                let omitted = lines.len() - rule.max_lines;
                Some(format!("{kept}\n... ({omitted} lines omitted)"))
            }
        }
        FilterMode::MatchLines => {
            if exit_code != 0 {
                return Some(output.to_string());
            }
            let Some(ref pat) = rule.match_pattern else {
                return Some(output.to_string());
            };
            let Ok(re) = regex::Regex::new(pat) else {
                return Some(output.to_string());
            };
            let matched: Vec<&str> = output.lines().filter(|l| re.is_match(l)).collect();
            if matched.is_empty() {
                None
            } else {
                Some(matched.join("\n"))
            }
        }
    }
}

/// Run the filter hook logic.
///
/// Returns the hook result indicating passthrough, replace, or suppress.
pub fn run_filter(payload: &FilterPayload, config: &FiltersConfig) -> FilterResult {
    let Some(rule) = find_rule(&payload.command, config) else {
        return FilterResult::Passthrough;
    };

    match apply_filter(&payload.output, payload.exit_code, rule) {
        Some(text) if text == payload.output => FilterResult::Passthrough,
        Some(text) => FilterResult::Replace(text),
        None => FilterResult::Suppress,
    }
}

/// The outcome of applying `filters.toml` to one command's output.
///
/// This exists so callers never have to treat an empty string as a sentinel for
/// "suppressed". `Option<String>` forces that reading: `None` means "no rule changed
/// anything", but `Some("")` then has to mean "suppress" — two different instructions
/// encoded in one type, and easy to conflate. `cmd_hook` and `opencode::run_hook` both
/// consume this, so the ambiguity was paid for twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapedOutput {
    /// No rule matched, or the matching rule produced text identical to the input. Callers
    /// keep whatever they already had and emit no replacement.
    Unchanged,
    /// A rule matched and suppressed the output entirely.
    Suppressed,
    /// A rule matched and produced this replacement text.
    Replaced(String),
}

impl ShapedOutput {
    /// The text to hand downstream as this command's output, or `None` to keep the original.
    pub fn text(&self) -> Option<String> {
        match self {
            Self::Unchanged => None,
            Self::Suppressed => Some(String::new()),
            Self::Replaced(text) => Some(text.clone()),
        }
    }
}

/// Apply the first matching filter rule to `output`.
pub fn shape_command_output(
    config: &FiltersConfig,
    command: &str,
    output: &str,
    exit_code: i64,
) -> ShapedOutput {
    let payload = FilterPayload {
        command: command.to_string(),
        output: output.to_string(),
        exit_code,
    };
    match run_filter(&payload, config) {
        FilterResult::Passthrough => ShapedOutput::Unchanged,
        FilterResult::Replace(text) => ShapedOutput::Replaced(text),
        FilterResult::Suppress => ShapedOutput::Suppressed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rule(mode: FilterMode, max_lines: usize) -> FilterRule {
        FilterRule {
            pattern: "cargo nextest".to_string(),
            mode,
            max_lines,
            match_pattern: None,
        }
    }

    fn cfg_with(mode: FilterMode) -> FiltersConfig {
        FiltersConfig {
            filters: vec![FilterRule {
                pattern: "cargo nextest".to_string(),
                mode,
                max_lines: 3,
                match_pattern: None,
            }],
            ..Default::default()
        }
    }

    fn payload(cmd: &str, output: &str, exit_code: i64) -> FilterPayload {
        FilterPayload {
            command: cmd.to_string(),
            output: output.to_string(),
            exit_code,
        }
    }

    #[test]
    fn passthrough_returns_output_unchanged() {
        let rule = make_rule(FilterMode::Passthrough, 50);
        assert_eq!(
            apply_filter("some output", 0, &rule),
            Some("some output".to_string())
        );
    }

    #[test]
    fn failures_only_suppresses_on_success() {
        let rule = make_rule(FilterMode::FailuresOnly, 50);
        assert_eq!(apply_filter("output", 0, &rule), None);
    }

    #[test]
    fn failures_only_passes_on_failure() {
        let rule = make_rule(FilterMode::FailuresOnly, 50);
        assert_eq!(
            apply_filter("error output", 1, &rule),
            Some("error output".to_string())
        );
    }

    #[test]
    fn errors_only_filters_to_error_lines() {
        let rule = make_rule(FilterMode::ErrorsOnly, 50);
        let output = "line 1\nerror: something failed\nline 3\nERROR: another\n";
        let result = apply_filter(output, 0, &rule).unwrap();
        assert!(result.contains("error: something failed"));
        assert!(result.contains("ERROR: another"));
        assert!(!result.contains("line 1"));
    }

    #[test]
    fn truncate_keeps_first_n_lines() {
        let rule = make_rule(FilterMode::Truncate, 2);
        let output = "line1\nline2\nline3\nline4";
        let result = apply_filter(output, 0, &rule).unwrap();
        assert!(result.starts_with("line1\nline2"));
        assert!(result.contains("2 lines omitted"));
    }

    #[test]
    fn run_filter_passthrough_on_no_matching_rule() {
        let cfg = cfg_with(FilterMode::FailuresOnly);
        let p = payload("doob todo list", "output", 0);
        assert_eq!(run_filter(&p, &cfg), FilterResult::Passthrough);
    }

    #[test]
    fn run_filter_suppress_on_failures_only_success() {
        let cfg = cfg_with(FilterMode::FailuresOnly);
        let p = payload("cargo nextest run", "test output", 0);
        assert_eq!(run_filter(&p, &cfg), FilterResult::Suppress);
    }

    #[test]
    fn run_filter_replace_on_truncate() {
        let cfg = cfg_with(FilterMode::Truncate);
        let output = "a\nb\nc\nd\ne";
        let p = payload("cargo nextest run", output, 0);
        match run_filter(&p, &cfg) {
            FilterResult::Replace(s) => {
                assert!(s.contains("2 lines omitted"));
            }
            other => panic!("expected Replace, got {other:?}"),
        }
    }

    // --- shape_command_output -------------------------------------------------
    // The contract both hook paths depend on: Unchanged means "no rule changed anything",
    // Suppressed and Replaced both hand downstream a replacement.

    #[test]
    fn shape_is_unchanged_when_no_rule_matches() {
        let cfg = cfg_with(FilterMode::FailuresOnly);
        assert_eq!(
            shape_command_output(&cfg, "ls -la", "total 8", 0),
            ShapedOutput::Unchanged
        );
    }

    #[test]
    fn shape_is_suppressed_for_a_suppressing_rule() {
        let cfg = cfg_with(FilterMode::FailuresOnly);
        assert_eq!(
            shape_command_output(&cfg, "cargo nextest run", "3 passed", 0),
            ShapedOutput::Suppressed
        );
    }

    #[test]
    fn shape_is_replaced_for_a_transforming_rule() {
        let cfg = cfg_with(FilterMode::Truncate);
        let shaped = shape_command_output(&cfg, "cargo nextest run", "a\nb\nc\nd\ne", 0);
        assert!(matches!(shaped, ShapedOutput::Replaced(ref t) if t.contains("2 lines omitted")));
    }

    /// A rule that matches and passes text through unchanged is `Unchanged` — from the
    /// caller's perspective it is indistinguishable from no match, and behaves the same.
    #[test]
    fn shape_is_unchanged_when_matched_rule_returns_identical_text() {
        let cfg = cfg_with(FilterMode::FailuresOnly);
        assert_eq!(
            shape_command_output(&cfg, "cargo nextest run", "1 failed", 1),
            ShapedOutput::Unchanged
        );
    }

    // --- ShapedOutput::text ---------------------------------------------------

    #[test]
    fn unchanged_yields_no_text() {
        assert_eq!(ShapedOutput::Unchanged.text(), None);
    }

    #[test]
    fn suppressed_yields_empty_text() {
        assert_eq!(ShapedOutput::Suppressed.text(), Some(String::new()));
    }

    #[test]
    fn replaced_yields_its_text() {
        assert_eq!(
            ShapedOutput::Replaced("shaped".into()).text(),
            Some("shaped".to_string())
        );
    }
}
