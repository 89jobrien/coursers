//! Domain types for output filters, rewrites, and tool swaps.

use serde::Deserialize;

const DEFAULT_MAX_LINES: usize = 50;

/// How to handle matched tool output.
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FilterMode {
    #[default]
    /// Pass output unchanged.
    Passthrough,
    /// Suppress output on success and preserve complete output on failure.
    FailuresOnly,
    /// Keep only lines containing `error`, case-insensitively.
    ErrorsOnly,
    /// Keep at most the configured number of lines.
    Truncate,
    /// Keep only lines matching the configured regular expression on success.
    ///
    /// Failures preserve complete output. A missing or invalid match expression also preserves the
    /// output unchanged.
    MatchLines,
}

/// A single filter rule matching one or more commands.
#[derive(Debug, Clone, Deserialize)]
pub struct FilterRule {
    /// Regex pattern matched against the full command string.
    pub pattern: String,
    pub mode: FilterMode,
    /// Maximum lines retained by [`FilterMode::Truncate`].
    #[serde(default = "default_max_lines")]
    pub max_lines: usize,
    /// Regex used by [`FilterMode::MatchLines`].
    #[serde(default)]
    pub match_pattern: Option<String>,
    // TODO(match-lines-flags): add typed regex flag control for match-lines filters (#49).
}

fn default_max_lines() -> usize {
    DEFAULT_MAX_LINES
}

/// Root of crs-filters.toml.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct FiltersConfig {
    #[serde(default)]
    pub filters: Vec<FilterRule>,
    #[serde(default)]
    pub tool_swap: ToolSwapConfig,
}

impl FiltersConfig {
    /// Load filter configuration from a specific path.
    ///
    /// Returns the default configuration when the file is missing or malformed. This compatibility
    /// helper will move to `coursers-adapters` after its first published release.
    #[cfg(feature = "core-compat")]
    // TODO(adapter-core-compat): deprecate in favor of coursers_adapters::FsFiltersLoader when
    // that crate is published, then remove this feature after one minor release.
    pub fn load_from(path: &std::path::Path) -> Self {
        let Ok(content) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        toml::from_str(&content).unwrap_or_default()
    }
}

/// Config for tool-swap behaviour.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ToolSwapConfig {
    /// Token budget for swapping a bare `cat` command to `Read`. Defaults to 4000.
    pub cat_token_limit: usize,
    /// Largest requested tail accepted for a `Read` swap. Defaults to 500 lines.
    pub tail_limit_max: usize,
    /// Largest find depth accepted for a `Glob` swap. Defaults to 10.
    pub find_depth_max: usize,
}

impl Default for ToolSwapConfig {
    fn default() -> Self {
        Self {
            cat_token_limit: 4000,
            tail_limit_max: 500,
            find_depth_max: 10,
        }
    }
}

/// A rewrite rule: if `pattern` matches the command, replace with `replace`.
#[derive(Debug, Clone, Deserialize)]
pub struct RewriteRule {
    /// Regex matched against the full command string.
    pub pattern: String,
    /// Replacement text, including optional regex capture references.
    pub replace: String,
}

/// Root of the `[rewrites]` section in crs-filters.toml.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct RewriteConfig {
    #[serde(default)]
    pub rewrites: Vec<RewriteRule>,
}

/// Result of applying a filter to command output.
#[derive(Debug, PartialEq)]
pub enum FilterResult {
    Passthrough,
    Replace(String),
    Suppress,
}

/// Input to the filter pipeline.
pub struct FilterPayload {
    pub command: String,
    pub output: String,
    pub exit_code: i64,
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_filter_and_tool_swap_config_deserializes() {
        let config: FiltersConfig = toml::from_str(
            r#"
                [[filters]]
                pattern = "cargo nextest"
                mode = "truncate"
                max_lines = 25

                [tool_swap]
                cat_token_limit = 2000
                tail_limit_max = 250
                find_depth_max = 5
            "#,
        )
        .unwrap();

        assert_eq!(config.filters.len(), 1);
        assert_eq!(config.filters[0].mode, FilterMode::Truncate);
        assert_eq!(config.filters[0].max_lines, 25);
        assert_eq!(config.tool_swap.cat_token_limit, 2000);
        assert_eq!(config.tool_swap.tail_limit_max, 250);
        assert_eq!(config.tool_swap.find_depth_max, 5);
    }

    #[test]
    fn canonical_rewrite_config_deserializes() {
        let config: RewriteConfig = toml::from_str(
            r#"
                [[rewrites]]
                pattern = "^git status$"
                replace = "git status --short"
            "#,
        )
        .unwrap();

        assert_eq!(config.rewrites.len(), 1);
        assert_eq!(config.rewrites[0].pattern, "^git status$");
        assert_eq!(config.rewrites[0].replace, "git status --short");
    }

    #[test]
    fn filter_mode_default_is_passthrough() {
        assert_eq!(FilterMode::default(), FilterMode::Passthrough);
    }

    #[test]
    fn filter_mode_deserializes_kebab_case() {
        let m: FilterMode = serde_json::from_str("\"failures-only\"").unwrap();
        assert_eq!(m, FilterMode::FailuresOnly);
        let m: FilterMode = serde_json::from_str("\"errors-only\"").unwrap();
        assert_eq!(m, FilterMode::ErrorsOnly);
        let m: FilterMode = serde_json::from_str("\"truncate\"").unwrap();
        assert_eq!(m, FilterMode::Truncate);
        let m: FilterMode = serde_json::from_str("\"match-lines\"").unwrap();
        assert_eq!(m, FilterMode::MatchLines);
    }

    #[test]
    fn filter_mode_rejects_unknown_variant() {
        let res: Result<FilterMode, _> = serde_json::from_str("\"bogus-mode\"");
        assert!(res.is_err());
    }

    #[test]
    fn filter_rule_uses_default_max_lines_when_absent() {
        let json = r#"{"pattern": "foo", "mode": "truncate"}"#;
        let rule: FilterRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.max_lines, 50);
        assert_eq!(rule.match_pattern, None);
    }

    #[test]
    fn filter_rule_respects_explicit_max_lines() {
        let json = r#"{"pattern": "foo", "mode": "truncate", "max_lines": 10}"#;
        let rule: FilterRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.max_lines, 10);
    }

    #[test]
    fn tool_swap_config_default_values() {
        let cfg = ToolSwapConfig::default();
        assert_eq!(cfg.cat_token_limit, 4000);
        assert_eq!(cfg.tail_limit_max, 500);
        assert_eq!(cfg.find_depth_max, 10);
    }

    #[test]
    fn tool_swap_config_deserializes_with_partial_overrides() {
        let json = r#"{"cat_token_limit": 100}"#;
        let cfg: ToolSwapConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.cat_token_limit, 100);
        // untouched fields fall back to Default impl
        assert_eq!(cfg.tail_limit_max, 500);
        assert_eq!(cfg.find_depth_max, 10);
    }

    #[test]
    fn filters_config_default_is_empty() {
        let cfg = FiltersConfig::default();
        assert!(cfg.filters.is_empty());
        assert_eq!(cfg.tool_swap.cat_token_limit, 4000);
    }

    #[test]
    fn filters_config_deserializes_from_json() {
        let json = r#"{"filters": [{"pattern": "cat .*", "mode": "errors-only"}]}"#;
        let cfg: FiltersConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.filters.len(), 1);
        assert_eq!(cfg.filters[0].mode, FilterMode::ErrorsOnly);
    }

    #[test]
    fn rewrite_config_default_is_empty() {
        let cfg = RewriteConfig::default();
        assert!(cfg.rewrites.is_empty());
    }

    #[test]
    fn rewrite_rule_deserializes() {
        let json = r#"{"pattern": "^cat ", "replace": "bat "}"#;
        let rule: RewriteRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.pattern, "^cat ");
        assert_eq!(rule.replace, "bat ");
    }

    #[test]
    fn filter_result_variants_compare_by_value() {
        assert_eq!(FilterResult::Suppress, FilterResult::Suppress);
        assert_ne!(FilterResult::Passthrough, FilterResult::Suppress);
        assert_eq!(
            FilterResult::Replace("a".into()),
            FilterResult::Replace("a".into())
        );
        assert_ne!(
            FilterResult::Replace("a".into()),
            FilterResult::Replace("b".into())
        );
    }
}

#[cfg(kani)]
mod kani_proofs {
    /// Proof: the default truncation bound is always positive.
    #[kani::proof]
    #[kani::unwind(1)]
    fn default_max_lines_positive() {
        assert!(super::DEFAULT_MAX_LINES > 0);
    }
}
