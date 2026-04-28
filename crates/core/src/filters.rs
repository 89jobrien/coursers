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
/// 1. `CRS_FILTERS` env var (explicit override)
/// 2. `.ctx/crs-filters.toml` walking up from CWD to filesystem root
/// 3. `~/.config/crs/filters.toml` (global fallback)
pub fn filters_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("CRS_FILTERS") {
        return Some(PathBuf::from(p));
    }

    if let Ok(mut dir) = std::env::current_dir() {
        loop {
            let candidate = dir.join(".ctx/crs-filters.toml");
            if candidate.exists() {
                return Some(candidate);
            }
            if !dir.pop() {
                break;
            }
        }
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
    fn walks_up_to_find_filters() {
        let tmp = tempfile::tempdir().unwrap();
        // Create .ctx/crs-filters.toml in the grandparent
        let ctx = tmp.path().join(".ctx");
        std::fs::create_dir_all(&ctx).unwrap();
        let toml_path = ctx.join("crs-filters.toml");
        std::fs::write(
            &toml_path,
            "[[filters]]\npattern = \"walk-test\"\nmode = \"passthrough\"\n",
        )
        .unwrap();

        // Create a nested child dir to start from
        let child = tmp.path().join("a").join("b");
        std::fs::create_dir_all(&child).unwrap();

        // Save and restore CWD
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(&child).unwrap();
        let result = filters_path();
        std::env::set_current_dir(&orig).unwrap();

        assert_eq!(
            result.unwrap().canonicalize().unwrap(),
            toml_path.canonicalize().unwrap()
        );
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

// ---------------------------------------------------------------------------
// Obfsck redaction filters
// ---------------------------------------------------------------------------

/// A single redaction rule: lines matching `pattern` are replaced with `[REDACTED]`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RedactRule {
    pub label: String,
    pub pattern: String,
}

/// Root of `.ctx/obfsck-filters.yaml`.
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct ObfsckFilters {
    #[serde(default)]
    pub filters: Vec<RedactRule>,
}

impl ObfsckFilters {
    /// Load from a specific path. Returns empty on missing or malformed file.
    pub fn load_from(path: &std::path::Path) -> Self {
        let Ok(content) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        serde_saphyr::from_str(&content).unwrap_or_default()
    }
}

/// Load `.ctx/obfsck-filters.yaml` if it exists, otherwise return empty.
pub fn load_obfsck_filters() -> ObfsckFilters {
    let path = std::path::Path::new(".ctx/obfsck-filters.yaml");
    ObfsckFilters::load_from(path)
}

/// Apply redaction rules to `output`. Lines matching any pattern are replaced with `[REDACTED]`.
pub fn apply_redaction(output: &str, filters: &ObfsckFilters) -> String {
    if filters.filters.is_empty() {
        return output.to_string();
    }

    // Compile patterns once; skip invalid regex.
    let compiled: Vec<regex::Regex> = filters
        .filters
        .iter()
        .filter_map(|r| regex::Regex::new(&r.pattern).ok())
        .collect();

    if compiled.is_empty() {
        return output.to_string();
    }

    let mut result = output
        .lines()
        .map(|line| {
            if compiled.iter().any(|re| re.is_match(line)) {
                "[REDACTED]"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Preserve trailing newline if original had one.
    if output.ends_with('\n') {
        result.push('\n');
    }

    result
}

#[cfg(test)]
mod redaction_tests {
    use super::*;

    fn make_obfsck_filters(patterns: &[(&str, &str)]) -> ObfsckFilters {
        ObfsckFilters {
            filters: patterns
                .iter()
                .map(|(label, pattern)| RedactRule {
                    label: label.to_string(),
                    pattern: pattern.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn apply_redaction_replaces_matching_line() {
        let filters = make_obfsck_filters(&[("api-key", r"sk-[A-Za-z0-9]{10,}")]);
        let output = "some output\nsk-abc1234567890 is a secret\nclean line";
        let result = apply_redaction(output, &filters);
        assert!(
            result.contains("[REDACTED]"),
            "matching line must be redacted"
        );
        assert!(
            result.contains("some output"),
            "non-matching lines must be preserved"
        );
        assert!(
            result.contains("clean line"),
            "non-matching lines must be preserved"
        );
        assert!(
            !result.contains("sk-abc1234567890"),
            "secret must not appear in output"
        );
    }

    #[test]
    fn apply_redaction_empty_filters_passthrough() {
        let filters = make_obfsck_filters(&[]);
        let output = "sk-abc1234567890 is a secret";
        let result = apply_redaction(output, &filters);
        assert_eq!(result, output);
    }

    #[test]
    fn apply_redaction_no_match_passthrough() {
        let filters = make_obfsck_filters(&[("api-key", r"sk-[A-Za-z0-9]{10,}")]);
        let output = "no secrets here";
        let result = apply_redaction(output, &filters);
        assert_eq!(result, output);
    }

    #[test]
    fn load_obfsck_filters_missing_file_returns_empty() {
        let filters = ObfsckFilters::load_from(std::path::Path::new("/nonexistent/path.yaml"));
        assert!(filters.filters.is_empty());
    }

    #[test]
    fn load_obfsck_filters_parses_yaml() {
        use std::io::Write as _;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            "filters:\n  - label: test\n    pattern: \"secret-[0-9]+\"\n"
        )
        .unwrap();
        let filters = ObfsckFilters::load_from(f.path());
        assert_eq!(filters.filters.len(), 1);
        assert_eq!(filters.filters[0].label, "test");
        assert_eq!(filters.filters[0].pattern, "secret-[0-9]+");
    }

    #[test]
    fn apply_redaction_preserves_trailing_newline() {
        let filters = make_obfsck_filters(&[("key", r"secret")]);
        let output = "clean\nsecret line\n";
        let result = apply_redaction(output, &filters);
        assert!(result.ends_with('\n'), "trailing newline must be preserved");
        assert!(result.contains("[REDACTED]"));
    }
}
