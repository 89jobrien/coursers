use serde::Deserialize;

/// A rewrite rule: if `pattern` matches the command, replace with `replace`.
#[derive(Debug, Clone, Deserialize)]
pub struct RewriteRule {
    /// Regex matched against the full command string.
    pub pattern: String,
    /// Replacement string (may use regex capture groups: `$1`, `$2`, ...).
    pub replace: String,
}

/// Root of the `[rewrites]` section in crs-filters.toml.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct RewriteConfig {
    #[serde(default)]
    pub rewrites: Vec<RewriteRule>,
}

/// Try to rewrite `command` using the first matching rule.
///
/// The command is first passed through [`crate::expand::expand_vars`] to resolve
/// shell env references (`$HOME`, `${VAR}`, `$env.VAR`, `~`) before rule matching.
/// If a rule matches, the *expanded* form is rewritten and returned.
/// If no rule matches but expansion changed the command, the expanded form is returned
/// (so that env refs are always resolved even without an explicit rewrite rule).
///
/// Returns `Some(rewritten_or_expanded)` if the command changed, `None` if unchanged.
pub fn apply(command: &str, config: &RewriteConfig) -> Option<String> {
    let expanded = crate::expand::expand_vars(command);

    for rule in &config.rewrites {
        let Ok(re) = regex::Regex::new(&rule.pattern) else {
            continue;
        };
        if re.is_match(&expanded) {
            let result = re.replace(&expanded, rule.replace.as_str()).into_owned();
            return Some(result);
        }
    }

    // No rule matched. Return expanded form if it differs from the original.
    if expanded != command {
        Some(expanded)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(rules: &[(&str, &str)]) -> RewriteConfig {
        RewriteConfig {
            rewrites: rules
                .iter()
                .map(|(p, r)| RewriteRule {
                    pattern: p.to_string(),
                    replace: r.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn returns_none_on_no_match() {
        let c = cfg(&[("cargo nextest", "cargo nextest run")]);
        assert!(apply("doob todo list", &c).is_none());
    }

    #[test]
    fn rewrites_matching_command() {
        let c = cfg(&[("^git status$", "git status --short")]);
        assert_eq!(apply("git status", &c).unwrap(), "git status --short");
    }

    #[test]
    fn uses_first_matching_rule() {
        let c = cfg(&[
            ("^cargo nextest.*", "cargo nextest run --no-fail-fast"),
            ("^cargo.*", "cargo --color always"),
        ]);
        assert_eq!(
            apply("cargo nextest run", &c).unwrap(),
            "cargo nextest run --no-fail-fast"
        );
    }

    #[test]
    fn supports_capture_groups() {
        let c = cfg(&[("^(cargo test)(.*)", "cargo nextest run$2")]);
        assert_eq!(
            apply("cargo test --release", &c).unwrap(),
            "cargo nextest run --release"
        );
    }

    #[test]
    fn passthrough_on_empty_rules() {
        let c = RewriteConfig::default();
        assert!(apply("anything", &c).is_none());
    }

    #[test]
    fn invalid_regex_skipped() {
        let c = cfg(&[("[(invalid", "replace"), ("^cargo build$", "cargo --color always build")]);
        assert_eq!(apply("cargo build", &c).unwrap(), "cargo --color always build");
    }
}
