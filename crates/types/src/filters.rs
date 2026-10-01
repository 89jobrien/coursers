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
