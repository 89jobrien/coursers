//! Generic hook pipeline — declarative rules for all Claude Code hook events.
//!
//! Rules are loaded from TOML config files and evaluated in order. The first
//! `Deny` wins; rewrites and side-effects accumulate.

use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Claude Code hook events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    SessionStart,
    SessionEnd,
    PreCompact,
    Stop,
    SubagentStop,
}

/// When a post-hook rule should fire relative to the tool's exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum When {
    #[default]
    Always,
    OnSuccess,
    OnFailure,
}

/// What a hook rule does when it matches.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum HookAction {
    /// Block the tool call with a message.
    Deny { message: String },
    /// Rewrite the command (PreToolUse/Bash only).
    Rewrite {
        /// If set, inject this string into the command.
        #[serde(default)]
        inject: Option<String>,
        /// If set, prepend this to the command.
        #[serde(default)]
        prepend: Option<String>,
        /// If set, replace the entire command via regex substitution.
        #[serde(default)]
        replace: Option<String>,
    },
    /// Run an external command as a side-effect.
    Run {
        command: Vec<String>,
        /// If true, capture stdout and emit as a system message.
        #[serde(default)]
        capture: bool,
    },
    /// Emit a system message (informational, non-blocking).
    Notify { template: String },
}

/// A single hook rule.
#[derive(Debug, Clone, Deserialize)]
pub struct HookRule {
    /// Which event this rule fires on.
    pub event: HookEvent,
    /// Tool name glob filter (e.g. "Bash", "Edit|Write", "*").
    /// Only meaningful for PreToolUse/PostToolUse.
    #[serde(default)]
    pub matcher: Option<String>,
    /// Regex pattern matched against the command (Bash) or file_path (Edit/Write).
    #[serde(default)]
    pub pattern: Option<String>,
    /// Skip this rule if the command/path contains this string.
    #[serde(default)]
    pub unless: Option<String>,
    /// When to fire (PostToolUse only).
    #[serde(default)]
    pub when: When,
    /// The action to take.
    #[serde(flatten)]
    pub action: HookAction,
    /// Human-readable label for logging/diagnostics.
    #[serde(default)]
    pub label: Option<String>,
}

// ---------------------------------------------------------------------------
// Config loading
// ---------------------------------------------------------------------------

/// Root config shape for `crs-hooks.toml` / plugin TOML files.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct HookPipelineConfig {
    #[serde(default)]
    pub hooks: Vec<HookRule>,
}

impl HookPipelineConfig {
    pub fn load_from(path: &std::path::Path) -> Self {
        let Ok(content) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        toml::from_str(&content).unwrap_or_default()
    }

    /// Merge another config's rules after ours.
    pub fn merge(&mut self, other: Self) {
        self.hooks.extend(other.hooks);
    }
}

/// Load the full hook pipeline config by merging all sources:
/// 1. Project-local `.ctx/crs-hooks.toml` (walk up from CWD)
/// 2. Global `~/.config/crs/hooks.toml`
/// 3. Plugin configs from `~/.config/crs/plugins.d/*.toml`
pub fn load_config() -> HookPipelineConfig {
    let mut config = HookPipelineConfig::default();

    // 1. Global config
    if let Some(home) = dirs::home_dir() {
        let global = home.join(".config/crs/hooks.toml");
        if global.exists() {
            config.merge(HookPipelineConfig::load_from(&global));
        }
    }

    // 2. Plugin configs
    if let Some(home) = dirs::home_dir() {
        let plugins_dir = home.join(".config/crs/plugins.d");
        if plugins_dir.is_dir()
            && let Ok(entries) = std::fs::read_dir(&plugins_dir)
        {
            let mut paths: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "toml"))
                .collect();
            paths.sort();
            for path in paths {
                config.merge(HookPipelineConfig::load_from(&path));
            }
        }
    }

    // 3. Project-local (highest priority — appended last so it can override)
    if let Some(path) = find_project_hooks_toml() {
        config.merge(HookPipelineConfig::load_from(&path));
    }

    config
}

fn find_hooks_toml_from(start: &std::path::Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        let candidate = dir.join(".ctx/crs-hooks.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

fn find_project_hooks_toml() -> Option<PathBuf> {
    find_hooks_toml_from(&std::env::current_dir().ok()?)
}

// ---------------------------------------------------------------------------
// Context — the data available to rules at evaluation time
// ---------------------------------------------------------------------------

/// Everything a hook rule might need to match against or act on.
#[derive(Debug, Clone, Default)]
pub struct HookContext {
    pub event: Option<HookEvent>,
    pub tool_name: Option<String>,
    /// For Bash: the command. For Edit/Write: the file_path.
    pub target: Option<String>,
    pub exit_code: Option<i64>,
    /// Raw stdin JSON, available for side-effect commands.
    pub raw_json: Option<String>,
}

// ---------------------------------------------------------------------------
// Pipeline execution
// ---------------------------------------------------------------------------

/// Result of running the pipeline for a single event.
#[derive(Debug, Default)]
pub struct PipelineResult {
    /// If set, the tool call should be denied with this message.
    pub deny: Option<String>,
    /// If set, the command/input should be rewritten to this.
    pub rewrite: Option<String>,
    /// System messages to emit (from Notify actions or captured Run output).
    pub messages: Vec<String>,
    /// Labels of rules that matched (for logging).
    pub matched_rules: Vec<String>,
}

/// Run all rules matching `ctx.event` against the given context.
pub fn run_pipeline(config: &HookPipelineConfig, ctx: &HookContext) -> PipelineResult {
    let Some(event) = ctx.event else {
        return PipelineResult::default();
    };

    let mut result = PipelineResult::default();
    let target = ctx.target.as_deref().unwrap_or("");

    for rule in &config.hooks {
        if rule.event != event {
            continue;
        }

        // Matcher filter (tool_name)
        if let Some(ref matcher) = rule.matcher {
            let tool = ctx.tool_name.as_deref().unwrap_or("");
            if !matches_tool(matcher, tool) {
                continue;
            }
        }

        // Pattern filter
        if let Some(ref pattern) = rule.pattern {
            let Ok(re) = regex::Regex::new(pattern) else {
                continue;
            };
            if !re.is_match(target) {
                continue;
            }
        }

        // Unless filter
        if let Some(ref unless) = rule.unless
            && target.contains(unless.as_str())
        {
            continue;
        }

        // When filter (PostToolUse only)
        match rule.when {
            When::OnSuccess => {
                if ctx.exit_code.unwrap_or(0) != 0 {
                    continue;
                }
            }
            When::OnFailure => {
                if ctx.exit_code.unwrap_or(0) == 0 {
                    continue;
                }
            }
            When::Always => {}
        }

        // Track matched rule
        let label = rule
            .label
            .clone()
            .unwrap_or_else(|| format!("rule-{}", result.matched_rules.len()));
        result.matched_rules.push(label);

        // Execute action
        match &rule.action {
            HookAction::Deny { message } => {
                let expanded = expand_template(message, ctx);
                result.deny = Some(expanded);
                // First deny wins — stop processing.
                return result;
            }
            HookAction::Rewrite {
                inject,
                prepend,
                replace,
            } => {
                let current = result.rewrite.as_deref().unwrap_or(target).to_string();
                let rewritten = apply_rewrite(
                    &current,
                    inject.as_deref(),
                    prepend.as_deref(),
                    replace.as_deref(),
                );
                if rewritten != current {
                    result.rewrite = Some(rewritten);
                }
            }
            HookAction::Run { command, capture } => {
                run_side_effect(command, *capture, ctx, &mut result);
            }
            HookAction::Notify { template } => {
                let expanded = expand_template(template, ctx);
                result.messages.push(expanded);
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Match a tool name against a matcher pattern like "Bash", "Edit|Write", "*".
fn matches_tool(matcher: &str, tool: &str) -> bool {
    if matcher == "*" {
        return true;
    }
    matcher.split('|').any(|m| m.trim() == tool)
}

/// Apply rewrite transforms to a command string.
fn apply_rewrite(
    command: &str,
    inject: Option<&str>,
    prepend: Option<&str>,
    replace: Option<&str>,
) -> String {
    let mut result = command.to_string();

    if let Some(prepend) = prepend {
        let expanded = expand_env_vars(prepend);
        result = format!("{expanded} {result}");
    }

    if let Some(inject) = inject {
        let expanded = expand_env_vars(inject);
        result = format!("{result} {expanded}");
    }

    if let Some(replace) = replace {
        // `replace` is treated as a regex replacement string.
        // The rule's `pattern` already matched, so we apply the replacement
        // to the full command.
        if let Ok(re) = regex::Regex::new(replace) {
            result = re.replace_all(&result, replace).to_string();
        }
    }

    result
}

/// Expand `${VAR}` and `$VAR` in a string from the environment.
fn expand_env_vars(s: &str) -> String {
    let mut result = s.to_string();

    // Handle ${GIT_BRANCH_SLUG} specially — compute on demand.
    if result.contains("${GIT_BRANCH_SLUG}") {
        let slug = git_branch_slug().unwrap_or_default();
        result = result.replace("${GIT_BRANCH_SLUG}", &slug);
    }

    // Generic ${VAR} expansion.
    let re = regex::Regex::new(r"\$\{(\w+)\}").unwrap();
    result = re
        .replace_all(&result, |caps: &regex::Captures| {
            let var = &caps[1];
            std::env::var(var).unwrap_or_default()
        })
        .to_string();

    result
}

/// Expand `${target}`, `${tool_name}`, `${exit_code}` in a template.
fn expand_template(template: &str, ctx: &HookContext) -> String {
    let mut s = template.to_string();
    if let Some(ref t) = ctx.target {
        s = s.replace("${target}", t);
    }
    if let Some(ref t) = ctx.tool_name {
        s = s.replace("${tool_name}", t);
    }
    if let Some(code) = ctx.exit_code {
        s = s.replace("${exit_code}", &code.to_string());
    }
    s
}

/// Get the current git branch as a slug (strip prefix, replace / and _ with -).
fn git_branch_slug() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !branch.starts_with("feature/")
        && !branch.starts_with("fix/")
        && !branch.starts_with("chore/")
        && !branch.starts_with("feat/")
    {
        return None;
    }
    let slug = branch
        .split_once('/')
        .map(|(_, rest)| rest)
        .unwrap_or(&branch)
        .replace(['/', '_'], "-")
        .to_lowercase();
    Some(slug)
}

/// Run a side-effect command. Optionally capture stdout as a system message.
fn run_side_effect(
    args: &[String],
    capture: bool,
    _ctx: &HookContext,
    result: &mut PipelineResult,
) {
    let Some((program, cmd_args)) = args.split_first() else {
        return;
    };
    let output = Command::new(program).args(cmd_args).output();
    match output {
        Ok(out) if capture && out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !stdout.is_empty() {
                result.messages.push(stdout);
            }
        }
        _ => {} // fire-and-forget
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// A diagnostic from hook config validation.
#[derive(Debug)]
pub struct HookDiagnostic {
    pub level: DiagLevel,
    pub rule_index: usize,
    pub label: String,
    pub message: String,
}

/// Severity level for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagLevel {
    Error,
    Warning,
}

/// Structural validation: catches configs that cannot work at runtime.
pub fn validate_config(config: &HookPipelineConfig) -> Vec<HookDiagnostic> {
    let mut diags = Vec::new();

    for (i, rule) in config.hooks.iter().enumerate() {
        let label = rule.label.clone().unwrap_or_else(|| format!("hooks[{i}]"));

        // Pattern must compile
        if let Some(ref pat) = rule.pattern {
            if regex::Regex::new(pat).is_err() {
                diags.push(HookDiagnostic {
                    level: DiagLevel::Error,
                    rule_index: i,
                    label: label.clone(),
                    message: format!("invalid regex pattern: {pat}"),
                });
            }
        }

        // Unless must compile
        if let Some(ref pat) = rule.unless {
            if regex::Regex::new(pat).is_err() {
                diags.push(HookDiagnostic {
                    level: DiagLevel::Error,
                    rule_index: i,
                    label: label.clone(),
                    message: format!("invalid unless regex: {pat}"),
                });
            }
        }

        // Run action: command must not be empty
        if let HookAction::Run { ref command, .. } = rule.action {
            if command.is_empty() || command.iter().all(|c| c.trim().is_empty()) {
                diags.push(HookDiagnostic {
                    level: DiagLevel::Error,
                    rule_index: i,
                    label: label.clone(),
                    message: "run action has empty command".into(),
                });
            }
        }
    }

    // Sort: errors first
    diags.sort_by_key(|d| match d.level {
        DiagLevel::Error => 0,
        DiagLevel::Warning => 1,
    });
    diags
}

/// Optional style/convention lint checks. Separate from `validate_config` so
/// callers can opt in to stricter analysis.
pub fn lint_config(config: &HookPipelineConfig) -> Vec<HookDiagnostic> {
    let mut diags = Vec::new();

    for (i, rule) in config.hooks.iter().enumerate() {
        let label = rule.label.clone().unwrap_or_else(|| format!("hooks[{i}]"));

        // deny-by-default: deny without pattern catches everything
        if matches!(rule.action, HookAction::Deny { .. }) && rule.pattern.is_none() {
            diags.push(HookDiagnostic {
                level: DiagLevel::Warning,
                rule_index: i,
                label: label.clone(),
                message: "deny-by-default: deny rule has no pattern, blocks all matching commands for this event"
                    .into(),
            });
        }

        // Notify with empty template is useless
        if let HookAction::Notify { ref template } = rule.action {
            if template.trim().is_empty() {
                diags.push(HookDiagnostic {
                    level: DiagLevel::Error,
                    rule_index: i,
                    label: label.clone(),
                    message: "notify action has empty template".into(),
                });
            }
        }

        // Label namespacing convention: should contain '/'
        if let Some(ref l) = rule.label {
            if !l.contains('/') {
                diags.push(HookDiagnostic {
                    level: DiagLevel::Warning,
                    rule_index: i,
                    label: label.clone(),
                    message:
                        "label should use namespace/name convention (e.g. \"guardian/force-push\")"
                            .into(),
                });
            }
        }
    }

    diags
}

/// Collect all config source file paths that would be loaded.
pub fn config_source_paths() -> Vec<(String, PathBuf)> {
    let mut sources = Vec::new();

    if let Some(home) = dirs::home_dir() {
        let global = home.join(".config/crs/hooks.toml");
        if global.exists() {
            sources.push(("global".into(), global));
        }
    }

    if let Some(path) = find_project_hooks_toml() {
        sources.push(("project".into(), path));
    }

    sources
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(event: HookEvent, action: HookAction) -> HookRule {
        HookRule {
            event,
            matcher: None,
            pattern: None,
            unless: None,
            when: When::Always,
            action,
            label: None,
        }
    }

    fn ctx(event: HookEvent, target: &str) -> HookContext {
        HookContext {
            event: Some(event),
            tool_name: Some("Bash".into()),
            target: Some(target.into()),
            exit_code: Some(0),
            raw_json: None,
        }
    }

    #[test]
    fn deny_stops_pipeline() {
        let config = HookPipelineConfig {
            hooks: vec![
                rule(
                    HookEvent::PreToolUse,
                    HookAction::Deny {
                        message: "blocked".into(),
                    },
                ),
                rule(
                    HookEvent::PreToolUse,
                    HookAction::Notify {
                        template: "should not reach".into(),
                    },
                ),
            ],
        };
        let result = run_pipeline(&config, &ctx(HookEvent::PreToolUse, "git push --force"));
        assert_eq!(result.deny.as_deref(), Some("blocked"));
        assert!(result.messages.is_empty());
    }

    #[test]
    fn pattern_filters_correctly() {
        let config = HookPipelineConfig {
            hooks: vec![HookRule {
                pattern: Some(r"git\s+push.+--force".into()),
                ..rule(
                    HookEvent::PreToolUse,
                    HookAction::Deny {
                        message: "no force push".into(),
                    },
                )
            }],
        };
        // Should match
        let r = run_pipeline(
            &config,
            &ctx(HookEvent::PreToolUse, "git push origin --force"),
        );
        assert!(r.deny.is_some());
        // Should not match
        let r = run_pipeline(&config, &ctx(HookEvent::PreToolUse, "git push origin main"));
        assert!(r.deny.is_none());
    }

    #[test]
    fn unless_skips_rule() {
        let config = HookPipelineConfig {
            hooks: vec![HookRule {
                pattern: Some(r"doob todo add".into()),
                unless: Some("--tags".into()),
                ..rule(
                    HookEvent::PreToolUse,
                    HookAction::Notify {
                        template: "missing tags".into(),
                    },
                )
            }],
        };
        let r = run_pipeline(
            &config,
            &ctx(HookEvent::PreToolUse, "doob todo add --tags foo"),
        );
        assert!(r.messages.is_empty());

        let r = run_pipeline(
            &config,
            &ctx(HookEvent::PreToolUse, "doob todo add fix bug"),
        );
        assert_eq!(r.messages.len(), 1);
    }

    #[test]
    fn when_on_success_filters_by_exit_code() {
        let config = HookPipelineConfig {
            hooks: vec![HookRule {
                when: When::OnSuccess,
                ..rule(
                    HookEvent::PostToolUse,
                    HookAction::Notify {
                        template: "success".into(),
                    },
                )
            }],
        };
        let mut c = ctx(HookEvent::PostToolUse, "cargo test");
        c.exit_code = Some(0);
        assert_eq!(run_pipeline(&config, &c).messages.len(), 1);

        c.exit_code = Some(1);
        assert!(run_pipeline(&config, &c).messages.is_empty());
    }

    #[test]
    fn matcher_filters_tool_name() {
        let config = HookPipelineConfig {
            hooks: vec![HookRule {
                matcher: Some("Edit|Write".into()),
                ..rule(
                    HookEvent::PreToolUse,
                    HookAction::Deny {
                        message: "blocked".into(),
                    },
                )
            }],
        };
        let mut c = ctx(HookEvent::PreToolUse, "some/file.rs");
        c.tool_name = Some("Edit".into());
        assert!(run_pipeline(&config, &c).deny.is_some());

        c.tool_name = Some("Bash".into());
        assert!(run_pipeline(&config, &c).deny.is_none());
    }

    #[test]
    fn rewrite_prepend() {
        let config = HookPipelineConfig {
            hooks: vec![HookRule {
                pattern: Some(r"cargo nextest".into()),
                ..rule(
                    HookEvent::PreToolUse,
                    HookAction::Rewrite {
                        inject: None,
                        prepend: Some("_DEVLOOP_OP_WRAPPED=1".into()),
                        replace: None,
                    },
                )
            }],
        };
        let r = run_pipeline(
            &config,
            &ctx(HookEvent::PreToolUse, "cargo nextest run --workspace"),
        );
        assert_eq!(
            r.rewrite.as_deref(),
            Some("_DEVLOOP_OP_WRAPPED=1 cargo nextest run --workspace")
        );
    }

    #[test]
    fn event_mismatch_skips_rule() {
        let config = HookPipelineConfig {
            hooks: vec![rule(
                HookEvent::SessionStart,
                HookAction::Notify {
                    template: "hello".into(),
                },
            )],
        };
        let r = run_pipeline(&config, &ctx(HookEvent::PreToolUse, "anything"));
        assert!(r.messages.is_empty());
    }

    #[test]
    fn empty_config_returns_default() {
        let config = HookPipelineConfig::default();
        let r = run_pipeline(&config, &ctx(HookEvent::PreToolUse, "anything"));
        assert!(r.deny.is_none());
        assert!(r.rewrite.is_none());
        assert!(r.messages.is_empty());
    }

    #[test]
    fn matches_tool_works() {
        assert!(matches_tool("*", "Bash"));
        assert!(matches_tool("Bash", "Bash"));
        assert!(matches_tool("Edit|Write", "Edit"));
        assert!(matches_tool("Edit|Write", "Write"));
        assert!(!matches_tool("Edit|Write", "Bash"));
        assert!(!matches_tool("Bash", "Edit"));
    }

    #[test]
    fn config_merge() {
        let mut a = HookPipelineConfig {
            hooks: vec![rule(
                HookEvent::PreToolUse,
                HookAction::Deny {
                    message: "a".into(),
                },
            )],
        };
        let b = HookPipelineConfig {
            hooks: vec![rule(
                HookEvent::PostToolUse,
                HookAction::Notify {
                    template: "b".into(),
                },
            )],
        };
        a.merge(b);
        assert_eq!(a.hooks.len(), 2);
    }

    #[test]
    fn template_expansion() {
        let c = HookContext {
            event: Some(HookEvent::PostToolUse),
            tool_name: Some("Bash".into()),
            target: Some("cargo test".into()),
            exit_code: Some(1),
            raw_json: None,
        };
        let expanded = expand_template(
            "Tool ${tool_name} ran '${target}' with exit ${exit_code}",
            &c,
        );
        assert_eq!(expanded, "Tool Bash ran 'cargo test' with exit 1");
    }

    #[test]
    fn load_from_missing_file() {
        let cfg = HookPipelineConfig::load_from(std::path::Path::new("/nonexistent/hooks.toml"));
        assert!(cfg.hooks.is_empty());
    }

    #[test]
    fn load_from_valid_toml() {
        use std::io::Write as _;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[[hooks]]
event = "pre-tool-use"
matcher = "Bash"
pattern = 'git\s+push'
action = "deny"
message = "no push"
"#
        )
        .unwrap();
        let cfg = HookPipelineConfig::load_from(f.path());
        assert_eq!(cfg.hooks.len(), 1);
        assert_eq!(cfg.hooks[0].event, HookEvent::PreToolUse);
        assert!(matches!(cfg.hooks[0].action, HookAction::Deny { .. }));
    }

    #[test]
    fn load_from_run_action() {
        use std::io::Write as _;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[[hooks]]
event = "post-tool-use"
matcher = "Bash"
pattern = "git commit"
when = "on-success"
action = "run"
command = ["doob", "todo", "complete-from-commit"]
"#
        )
        .unwrap();
        let cfg = HookPipelineConfig::load_from(f.path());
        assert_eq!(cfg.hooks.len(), 1);
        assert!(matches!(cfg.hooks[0].action, HookAction::Run { .. }));
        assert_eq!(cfg.hooks[0].when, When::OnSuccess);
    }

    #[test]
    fn load_from_rewrite_action() {
        use std::io::Write as _;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[[hooks]]
event = "pre-tool-use"
matcher = "Bash"
pattern = "cargo nextest"
action = "rewrite"
prepend = "WRAPPED=1"
"#
        )
        .unwrap();
        let cfg = HookPipelineConfig::load_from(f.path());
        assert_eq!(cfg.hooks.len(), 1);
        if let HookAction::Rewrite { prepend, .. } = &cfg.hooks[0].action {
            assert_eq!(prepend.as_deref(), Some("WRAPPED=1"));
        } else {
            panic!("expected Rewrite action");
        }
    }

    #[test]
    fn find_hooks_toml_from_root_returns_none() {
        // Starting at / must not panic and must return None — no .ctx/crs-hooks.toml at root.
        let result = find_hooks_toml_from(std::path::Path::new("/"));
        assert!(result.is_none());
    }
}
