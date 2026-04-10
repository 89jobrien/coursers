pub mod jsonl_source;
pub mod rtk;

use crs_core::filters::{FiltersConfig, FilterMode, FilterRule, find_rule};
use crs_core::rewrite::RewriteConfig;
#[cfg(test)]
use crs_core::rewrite::RewriteRule;

/// Apply filter to output based on mode and exit code.
/// Returns the (possibly modified) output string, or None to suppress entirely.
pub fn apply_filter(
    output: &str,
    exit_code: i64,
    rule: &FilterRule,
) -> Option<String> {
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
    }
}

/// Run the `crs filter` hook logic.
///
/// Returns the hook message to emit (for `decision: allow` with message), or
/// `None` to emit nothing (passthrough silently).
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

/// Run the `crs rewrite` hook logic. Returns `Some(rewritten)` or `None`.
pub fn run_rewrite(command: &str, config: &RewriteConfig) -> Option<String> {
    crs_core::rewrite::apply(command, config)
}

#[derive(Debug, PartialEq)]
pub enum FilterResult {
    /// No change — let Claude Code proceed normally.
    Passthrough,
    /// Replace output with this string in the hook message.
    Replace(String),
    /// Suppress output entirely (emit empty message).
    Suppress,
}

pub struct FilterPayload {
    pub command: String,
    pub output: String,
    pub exit_code: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rule(mode: FilterMode, max_lines: usize) -> FilterRule {
        FilterRule {
            pattern: "cargo nextest".to_string(),
            mode,
            max_lines,
        }
    }

    fn cfg_with(mode: FilterMode) -> FiltersConfig {
        FiltersConfig {
            filters: vec![FilterRule {
                pattern: "cargo nextest".to_string(),
                mode,
                max_lines: 3,
            }],
        }
    }

    fn payload(cmd: &str, output: &str, exit_code: i64) -> FilterPayload {
        FilterPayload {
            command: cmd.to_string(),
            output: output.to_string(),
            exit_code,
        }
    }

    // --- apply_filter ---

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
        assert!(!result.contains("line 3"));
    }

    #[test]
    fn errors_only_returns_none_when_no_errors() {
        let rule = make_rule(FilterMode::ErrorsOnly, 50);
        assert_eq!(apply_filter("line 1\nline 2\n", 0, &rule), None);
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
    fn truncate_passthrough_when_under_limit() {
        let rule = make_rule(FilterMode::Truncate, 10);
        let output = "line1\nline2";
        assert_eq!(apply_filter(output, 0, &rule), Some(output.to_string()));
    }

    // --- run_filter ---

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

    // --- run_rewrite ---

    #[test]
    fn run_rewrite_returns_none_on_no_match() {
        let cfg = RewriteConfig::default();
        assert!(run_rewrite("cargo build", &cfg).is_none());
    }

    #[test]
    fn run_rewrite_returns_rewritten_command() {
        let cfg = RewriteConfig {
            rewrites: vec![RewriteRule {
                pattern: "^git status$".to_string(),
                replace: "git status --short".to_string(),
            }],
        };
        assert_eq!(
            run_rewrite("git status", &cfg).unwrap(),
            "git status --short"
        );
    }
}
