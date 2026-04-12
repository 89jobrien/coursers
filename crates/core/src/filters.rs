use serde::Deserialize;
use std::path::PathBuf;

/// How to handle matched tool output.
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FilterMode {
    #[default]
    /// Pass output unchanged.
    Passthrough,
    /// Suppress output entirely on success (exit 0); pass through on failure.
    FailuresOnly,
    /// Only pass lines containing "error" (case-insensitive).
    ErrorsOnly,
    /// Truncate to `max_lines` lines.
    Truncate,
}

/// A single filter rule matching one or more commands.
#[derive(Debug, Clone, Deserialize)]
pub struct FilterRule {
    /// Regex pattern matched against the full command string.
    pub pattern: String,
    pub mode: FilterMode,
    /// Used with `Truncate` mode: maximum lines to keep.
    #[serde(default = "default_max_lines")]
    pub max_lines: usize,
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
    pub tool_swap: crate::tool_swap::ToolSwapConfig,
}

impl FiltersConfig {
    /// Load config from a specific path. Returns default (empty) on missing file.
    pub fn load_from(path: &std::path::Path) -> Self {
        let Ok(content) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        toml::from_str(&content).unwrap_or_default()
    }
}

/// Resolve the active filters config using the hierarchy:
/// 1. `.ctx/crs-filters.toml` (project-local, wins)
/// 2. `~/.config/crs/filters.toml` (global fallback)
pub fn filters_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("CRS_FILTERS") {
        return Some(PathBuf::from(p));
    }

    let local = std::path::Path::new(".ctx/crs-filters.toml");
    if local.exists() {
        return Some(local.to_path_buf());
    }

    let global = dirs::home_dir()?.join(".config/crs/filters.toml");
    if global.exists() {
        return Some(global);
    }

    None
}

/// Load the active filters config (project-local wins over global).
pub fn load() -> FiltersConfig {
    filters_path()
        .map(|p| FiltersConfig::load_from(&p))
        .unwrap_or_default()
}

/// Find the first matching filter rule for `command`.
pub fn find_rule<'a>(command: &str, config: &'a FiltersConfig) -> Option<&'a FilterRule> {
    config.filters.iter().find(|r| {
        regex::Regex::new(&r.pattern)
            .map(|re| re.is_match(command))
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn write_toml(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "{content}").unwrap();
        f
    }

    #[test]
    fn empty_file_returns_default() {
        let f = write_toml("");
        let cfg = FiltersConfig::load_from(f.path());
        assert!(cfg.filters.is_empty());
    }

    #[test]
    fn missing_file_returns_default() {
        let cfg = FiltersConfig::load_from(std::path::Path::new("/nonexistent/path.toml"));
        assert!(cfg.filters.is_empty());
    }

    #[test]
    fn parses_passthrough_rule() {
        let f = write_toml(
            r#"
[[filters]]
pattern = "cargo nextest"
mode = "passthrough"
"#,
        );
        let cfg = FiltersConfig::load_from(f.path());
        assert_eq!(cfg.filters.len(), 1);
        assert_eq!(cfg.filters[0].mode, FilterMode::Passthrough);
    }

    #[test]
    fn parses_failures_only_rule() {
        let f = write_toml(
            r#"
[[filters]]
pattern = "cargo (nextest|test)"
mode = "failures-only"
"#,
        );
        let cfg = FiltersConfig::load_from(f.path());
        assert_eq!(cfg.filters[0].mode, FilterMode::FailuresOnly);
    }

    #[test]
    fn parses_truncate_rule_with_custom_max_lines() {
        let f = write_toml(
            r#"
[[filters]]
pattern = "doob todo"
mode = "truncate"
max_lines = 20
"#,
        );
        let cfg = FiltersConfig::load_from(f.path());
        assert_eq!(cfg.filters[0].mode, FilterMode::Truncate);
        assert_eq!(cfg.filters[0].max_lines, 20);
    }

    #[test]
    fn default_max_lines_is_50() {
        let f = write_toml(
            r#"
[[filters]]
pattern = "nu"
mode = "truncate"
"#,
        );
        let cfg = FiltersConfig::load_from(f.path());
        assert_eq!(cfg.filters[0].max_lines, 50);
    }

    #[test]
    fn find_rule_returns_first_match() {
        let cfg = FiltersConfig {
            filters: vec![
                FilterRule {
                    pattern: "cargo nextest".to_string(),
                    mode: FilterMode::FailuresOnly,
                    max_lines: 50,
                },
                FilterRule {
                    pattern: "cargo".to_string(),
                    mode: FilterMode::Passthrough,
                    max_lines: 50,
                },
            ],
            ..Default::default()
        };
        let rule = find_rule("cargo nextest run", &cfg).unwrap();
        assert_eq!(rule.mode, FilterMode::FailuresOnly);
    }

    #[test]
    fn find_rule_returns_none_on_no_match() {
        let cfg = FiltersConfig {
            filters: vec![FilterRule {
                pattern: "cargo".to_string(),
                mode: FilterMode::Passthrough,
                max_lines: 50,
            }],
            ..Default::default()
        };
        assert!(find_rule("doob todo list", &cfg).is_none());
    }

    #[test]
    fn env_override_takes_precedence() {
        let f = write_toml(
            r#"
[[filters]]
pattern = "from-env"
mode = "errors-only"
"#,
        );
        unsafe {
            std::env::set_var("CRS_FILTERS", f.path().to_str().unwrap());
        }
        let path = filters_path();
        unsafe {
            std::env::remove_var("CRS_FILTERS");
        }
        assert_eq!(path.unwrap(), f.path());
    }
}
