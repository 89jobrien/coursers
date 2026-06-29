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
