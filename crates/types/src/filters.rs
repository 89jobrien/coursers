use serde::Deserialize;

/// How to handle matched tool output.
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FilterMode {
    #[default]
    Passthrough,
    FailuresOnly,
    ErrorsOnly,
    Truncate,
    MatchLines,
}

/// A single filter rule matching one or more commands.
#[derive(Debug, Clone, Deserialize)]
pub struct FilterRule {
    pub pattern: String,
    pub mode: FilterMode,
    #[serde(default = "default_max_lines")]
    pub max_lines: usize,
    #[serde(default)]
    pub match_pattern: Option<String>,
}

fn default_max_lines() -> usize {
    50
}

/// Root of crs-filters.toml.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct FiltersConfig {
    #[serde(default)]
    pub filters: Vec<FilterRule>,
    #[serde(default)]
    pub tool_swap: ToolSwapConfig,
}

/// Config for tool-swap behaviour.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ToolSwapConfig {
    pub cat_token_limit: usize,
    pub tail_limit_max: usize,
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
    pub pattern: String,
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
