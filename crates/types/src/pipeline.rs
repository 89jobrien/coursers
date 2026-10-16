//! Domain configuration and results for generic hook pipelines.

use serde::Deserialize;

// TODO(pipeline-types-deduplication): This module is an UNUSED duplicate of
// `coursers_core::hook::pipeline`, which is the copy the runtime actually uses
// (see `crates/coursers/src/opencode.rs:27`, which imports
// `coursers_core::hook_pipeline::{HookContext as PipelineContext, HookEvent}`).
// Zero imports of `coursers_types::pipeline` exist anywhere in the workspace.
//
// The two copies have already DIVERGED in both directions:
//   - core has `HookAction::Redact { level }` — absent here
//   - core has `HookContext::output` and `PipelineResult::replace_output`
//     — absent here
//   - here, `HookAction` has no `Redact` arm at all, so any code switching to
//     this module loses output redaction and fails to match non-exhaustive
//     patterns
//
// Pick one owner before either copy gains a consumer. If core wins, delete this
// module and drop the hook-schema-parity marker below; if this module wins, core
// must re-export from here and the `Redact` action plus the `output` /
// `replace_output` fields must be ported across first.
// TODO(hook-schema-parity): Keep schemas/crs-hooks.schema.json synchronized with every
// runtime action, regex semantic, and supported template placeholder.
// TODO(harness-neutral-contract): Define shared request, response, and protocol-adapter
// contracts for Claude, Codex, and OpenCode instead of target-specific response paths.
/// Claude Code hook events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    SessionStart,
    SessionEnd,
    PermissionRequest,
    PreCompact,
    PostCompact,
    UserPromptSubmit,
    SubagentStart,
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
