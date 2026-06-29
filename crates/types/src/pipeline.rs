use serde::Deserialize;

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
    Deny {
        message: String,
    },
    Rewrite {
        #[serde(default)]
        inject: Option<String>,
        #[serde(default)]
        prepend: Option<String>,
        #[serde(default)]
        replace: Option<String>,
    },
    Run {
        command: Vec<String>,
        #[serde(default)]
        capture: bool,
    },
    Notify {
        template: String,
    },
}

/// A single hook rule.
#[derive(Debug, Clone, Deserialize)]
pub struct HookRule {
    pub event: HookEvent,
    #[serde(default)]
    pub matcher: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub unless: Option<String>,
    #[serde(default)]
    pub when: When,
    #[serde(flatten)]
    pub action: HookAction,
    #[serde(default)]
    pub label: Option<String>,
}

/// Root config shape for `crs-hooks.toml`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct HookPipelineConfig {
    #[serde(default)]
    pub hooks: Vec<HookRule>,
}

/// Everything a hook rule might need to match against or act on.
#[derive(Debug, Clone, Default)]
pub struct HookContext {
    pub event: Option<HookEvent>,
    pub tool_name: Option<String>,
    pub target: Option<String>,
    pub exit_code: Option<i64>,
    pub raw_json: Option<String>,
}

/// Result of running the pipeline for a single event.
#[derive(Debug, Default)]
pub struct PipelineResult {
    pub deny: Option<String>,
    pub rewrite: Option<String>,
    pub messages: Vec<String>,
    pub matched_rules: Vec<String>,
}

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
