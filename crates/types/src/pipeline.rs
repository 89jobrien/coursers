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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_event_deserializes_all_kebab_variants() {
        let pairs = [
            ("\"pre-tool-use\"", HookEvent::PreToolUse),
            ("\"post-tool-use\"", HookEvent::PostToolUse),
            ("\"session-start\"", HookEvent::SessionStart),
            ("\"session-end\"", HookEvent::SessionEnd),
            ("\"permission-request\"", HookEvent::PermissionRequest),
            ("\"pre-compact\"", HookEvent::PreCompact),
            ("\"post-compact\"", HookEvent::PostCompact),
            ("\"user-prompt-submit\"", HookEvent::UserPromptSubmit),
            ("\"subagent-start\"", HookEvent::SubagentStart),
            ("\"stop\"", HookEvent::Stop),
            ("\"subagent-stop\"", HookEvent::SubagentStop),
        ];
        for (json, expected) in pairs {
            let parsed: HookEvent = serde_json::from_str(json).unwrap();
            assert_eq!(parsed, expected, "mismatch for {json}");
        }
    }

    #[test]
    fn hook_event_rejects_unknown_variant() {
        let res: Result<HookEvent, _> = serde_json::from_str("\"not-a-real-event\"");
        assert!(res.is_err());
    }

    #[test]
    fn when_default_is_always() {
        assert_eq!(When::default(), When::Always);
    }

    #[test]
    fn when_deserializes_kebab_case() {
        let w: When = serde_json::from_str("\"on-success\"").unwrap();
        assert_eq!(w, When::OnSuccess);
        let w: When = serde_json::from_str("\"on-failure\"").unwrap();
        assert_eq!(w, When::OnFailure);
    }

    #[test]
    fn hook_action_deny_deserializes_with_tag() {
        let json = r#"{"action": "deny", "message": "blocked"}"#;
        let action: HookAction = serde_json::from_str(json).unwrap();
        match action {
            HookAction::Deny { message } => assert_eq!(message, "blocked"),
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[test]
    fn hook_action_rewrite_defaults_optional_fields() {
        let json = r#"{"action": "rewrite"}"#;
        let action: HookAction = serde_json::from_str(json).unwrap();
        match action {
            HookAction::Rewrite {
                inject,
                prepend,
                replace,
            } => {
                assert!(inject.is_none());
                assert!(prepend.is_none());
                assert!(replace.is_none());
            }
            other => panic!("expected Rewrite, got {other:?}"),
        }
    }

    #[test]
    fn hook_action_run_defaults_capture_to_false() {
        let json = r#"{"action": "run", "command": ["echo", "hi"]}"#;
        let action: HookAction = serde_json::from_str(json).unwrap();
        match action {
            HookAction::Run { command, capture } => {
                assert_eq!(command, vec!["echo".to_string(), "hi".to_string()]);
                assert!(!capture);
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn hook_action_notify_requires_template() {
        let json = r#"{"action": "notify", "template": "done: {{cmd}}"}"#;
        let action: HookAction = serde_json::from_str(json).unwrap();
        match action {
            HookAction::Notify { template } => assert_eq!(template, "done: {{cmd}}"),
            other => panic!("expected Notify, got {other:?}"),
        }
    }

    #[test]
    fn hook_action_unknown_tag_fails() {
        let json = r#"{"action": "explode"}"#;
        let res: Result<HookAction, _> = serde_json::from_str(json);
        assert!(res.is_err());
    }

    #[test]
    fn hook_rule_flattens_action_and_applies_defaults() {
        let json = r#"{
            "event": "pre-tool-use",
            "action": "deny",
            "message": "no"
        }"#;
        let rule: HookRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.event, HookEvent::PreToolUse);
        assert_eq!(rule.when, When::Always);
        assert!(rule.matcher.is_none());
        assert!(rule.pattern.is_none());
        assert!(rule.unless.is_none());
        assert!(rule.label.is_none());
        assert!(matches!(rule.action, HookAction::Deny { .. }));
    }

    #[test]
    fn hook_pipeline_config_default_is_empty() {
        let cfg = HookPipelineConfig::default();
        assert!(cfg.hooks.is_empty());
    }

    #[test]
    fn hook_pipeline_config_deserializes_multiple_rules() {
        let json = r#"{
            "hooks": [
                {"event": "pre-tool-use", "action": "deny", "message": "no"},
                {"event": "stop", "action": "notify", "template": "bye"}
            ]
        }"#;
        let cfg: HookPipelineConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.hooks.len(), 2);
        assert_eq!(cfg.hooks[1].event, HookEvent::Stop);
    }

    #[test]
    fn hook_context_default_is_all_none() {
        let ctx = HookContext::default();
        assert!(ctx.event.is_none());
        assert!(ctx.tool_name.is_none());
        assert!(ctx.target.is_none());
        assert!(ctx.exit_code.is_none());
        assert!(ctx.raw_json.is_none());
    }

    #[test]
    fn pipeline_result_default_is_empty() {
        let result = PipelineResult::default();
        assert!(result.deny.is_none());
        assert!(result.rewrite.is_none());
        assert!(result.messages.is_empty());
        assert!(result.matched_rules.is_empty());
    }

    #[test]
    fn diag_level_equality() {
        assert_eq!(DiagLevel::Error, DiagLevel::Error);
        assert_ne!(DiagLevel::Error, DiagLevel::Warning);
    }

    #[test]
    fn hook_diagnostic_holds_fields() {
        let diag = HookDiagnostic {
            level: DiagLevel::Warning,
            rule_index: 2,
            label: "rule-2".into(),
            message: "unused matcher".into(),
        };
        assert_eq!(diag.rule_index, 2);
        assert_eq!(diag.level, DiagLevel::Warning);
    }
}
