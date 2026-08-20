use serde::Deserialize;

use crate::error::CourserError;

/// Port: abstracts how the rewrite configuration is loaded.
pub trait RewriteLoader {
    fn load(&self) -> Result<RewriteConfig, CourserError>;
}

/// Loads [`RewriteConfig`] from the active filters file (same TOML as
/// `FiltersConfig`, but parsed for the `[[rewrites]]` tables only).
pub struct FsRewriteLoader;

impl RewriteLoader for FsRewriteLoader {
    fn load(&self) -> Result<RewriteConfig, CourserError> {
        let Some(content) = crate::hook::filters::merged_content() else {
            return Ok(RewriteConfig::default());
        };
        toml::from_str(&content).map_err(CourserError::Toml)
    }
}

/// In-memory rewrite loader for tests. Returns the config it was constructed with.
#[cfg(any(test, feature = "testing"))]
#[derive(Clone)]
pub struct InMemoryRewriteLoader(pub RewriteConfig);

#[cfg(any(test, feature = "testing"))]
impl RewriteLoader for InMemoryRewriteLoader {
    fn load(&self) -> Result<RewriteConfig, CourserError> {
        Ok(self.0.clone())
    }
}

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

/// Result of running a command through the rewrite pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteOutcome {
    /// The command after all matching rules were applied, in order.
    pub command: String,
    /// The `pattern` string of each rule that fired, in application order.
    pub applied_rules: Vec<String>,
}

/// Rewrite `command` by folding it through every matching rule, in file order.
///
/// The command is first passed through `expander` to resolve shell env references
/// (`$HOME`, `${VAR}`, `$env.VAR`, `~`) before rule matching. Pass [`crate::expand::EnvExpander`]
/// for production use or [`crate::expand::NoopExpander`] to skip expansion.
///
/// Every rule whose pattern matches the *current* (possibly already-rewritten) command
/// fires exactly once, in a single linear pass over `config.rewrites` — a rule is never
/// re-evaluated after it fires, and there is no re-scanning to a fixed point.
///
/// Returns `Some(outcome)` if the command changed (by any rule, or by expansion alone),
/// `None` if unchanged.
pub fn apply(
    command: &str,
    config: &RewriteConfig,
    expander: &impl crate::expand::VarExpander,
) -> Option<RewriteOutcome> {
    let expanded = expander.expand(command);

    let mut current = expanded.clone();
    let mut applied = Vec::new();
    for rule in &config.rewrites {
        let Ok(re) = regex::Regex::new(&rule.pattern) else {
            continue;
        };
        if re.is_match(&current) {
            current = re.replace(&current, rule.replace.as_str()).into_owned();
            applied.push(rule.pattern.clone());
        }
    }

    if !applied.is_empty() {
        return Some(RewriteOutcome {
            command: current,
            applied_rules: applied,
        });
    }

    // No rule matched. Return expanded form if it differs from the original.
    if expanded != command {
        Some(RewriteOutcome {
            command: expanded,
            applied_rules: Vec::new(),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expand::NoopExpander;

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
        assert!(apply("doob todo list", &c, &NoopExpander).is_none());
    }

    #[test]
    fn rewrites_matching_command() {
        let c = cfg(&[("^git status$", "git status --short")]);
        assert_eq!(
            apply("git status", &c, &NoopExpander).unwrap().command,
            "git status --short"
        );
    }

    #[test]
    fn applies_all_matching_rules_in_sequence() {
        let c = cfg(&[
            ("^cargo nextest.*", "cargo nextest run --no-fail-fast"),
            ("^cargo.*", "cargo --color always run --no-fail-fast"),
        ]);
        let outcome = apply("cargo nextest run", &c, &NoopExpander).unwrap();
        assert_eq!(outcome.command, "cargo --color always run --no-fail-fast");
        assert_eq!(
            outcome.applied_rules,
            vec!["^cargo nextest.*".to_string(), "^cargo.*".to_string()]
        );
    }

    #[test]
    fn supports_capture_groups() {
        let c = cfg(&[("^(cargo test)(.*)", "cargo nextest run$2")]);
        assert_eq!(
            apply("cargo test --release", &c, &NoopExpander)
                .unwrap()
                .command,
            "cargo nextest run --release"
        );
    }

    #[test]
    fn passthrough_on_empty_rules() {
        let c = RewriteConfig::default();
        assert!(apply("anything", &c, &NoopExpander).is_none());
    }

    #[test]
    fn each_rule_fires_at_most_once() {
        // Rule's own replacement text still matches its own pattern — must NOT
        // re-fire (single linear pass, not loop-to-convergence).
        let c = cfg(&[("cargo", "cargo cargo")]);
        let outcome = apply("cargo build", &c, &NoopExpander).unwrap();
        assert_eq!(outcome.command, "cargo cargo build");
        assert_eq!(outcome.applied_rules, vec!["cargo".to_string()]);
    }

    #[test]
    fn real_world_cargo_test_pipe_cascade() {
        // Shape mirrors the three real rules in ~/.config/crs/filters.toml that
        // exposed the bug: grep->rg, 2>&1 strip, and cargo test->nextest, in that
        // file order. All three must fire on one command.
        let c = cfg(&[
            (r"(\|\s*)grep(\s)", "${1}rg${2}"),
            (r"\s+2>&1", " out+err>"),
            (r"^(cargo\s+test\b)(.*)", "cargo nextest run$2"),
        ]);
        let outcome = apply(
            r#"cargo test --workspace 2>&1 | grep -E "FAILED|error""#,
            &c,
            &NoopExpander,
        )
        .unwrap();
        assert_eq!(
            outcome.command,
            r#"cargo nextest run --workspace out+err> | rg -E "FAILED|error""#
        );
        assert_eq!(outcome.applied_rules.len(), 3);
    }

    #[test]
    fn invalid_regex_skipped() {
        let c = cfg(&[
            ("[(invalid", "replace"),
            ("^cargo build$", "cargo --color always build"),
        ]);
        assert_eq!(
            apply("cargo build", &c, &NoopExpander).unwrap().command,
            "cargo --color always build"
        );
    }
}
