//! Domain types for hook contexts, outputs, and decisions.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Full Claude Code hook payload (PreToolUse or PostToolUse).
#[derive(Debug, Deserialize)]
pub struct HookPayload {
    pub tool_name: Option<String>,
    pub tool_input: Option<ToolInput>,
    pub tool_response: Option<Value>,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
}

/// The `tool_input` field of a Claude Code hook payload.
#[derive(Debug, Deserialize)]
pub struct ToolInput {
    pub command: Option<String>,
}

/// PreToolUse response envelope.
#[derive(Debug, Serialize)]
pub struct PreResponse {
    #[serde(rename = "hookSpecificOutput")]
    pub hook_specific_output: HookSpecificOutput,
}

/// Inner payload of a `PreToolUse` permission response.
#[derive(Debug, Serialize)]
pub struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    pub hook_event_name: String,
    #[serde(rename = "permissionDecision")]
    pub permission_decision: String,
    #[serde(rename = "permissionDecisionReason")]
    pub permission_decision_reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_payload_deserializes_full_shape() {
        let json = r#"{
            "tool_name": "Bash",
            "tool_input": {"command": "ls -la"},
            "tool_response": {"exit_code": 0},
            "session_id": "sess-1",
            "cwd": "/tmp"
        }"#;
        let payload: HookPayload = serde_json::from_str(json).unwrap();
        assert_eq!(payload.tool_name.as_deref(), Some("Bash"));
        assert_eq!(
            payload.tool_input.unwrap().command.as_deref(),
            Some("ls -la")
        );
        assert_eq!(payload.session_id.as_deref(), Some("sess-1"));
        assert_eq!(payload.cwd.as_deref(), Some("/tmp"));
    }

    #[test]
    fn hook_payload_deserializes_with_all_fields_absent() {
        let payload: HookPayload = serde_json::from_str("{}").unwrap();
        assert!(payload.tool_name.is_none());
        assert!(payload.tool_input.is_none());
        assert!(payload.tool_response.is_none());
        assert!(payload.session_id.is_none());
        assert!(payload.cwd.is_none());
    }

    #[test]
    fn tool_input_deserializes_without_command() {
        let input: ToolInput = serde_json::from_str("{}").unwrap();
        assert!(input.command.is_none());
    }

    #[test]
    fn pre_response_serializes_expected_json_keys() {
        let resp = PreResponse {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: "PreToolUse".into(),
                permission_decision: "deny".into(),
                permission_decision_reason: "blocked".into(),
            },
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(json["hookSpecificOutput"]["permissionDecision"], "deny");
        assert_eq!(
            json["hookSpecificOutput"]["permissionDecisionReason"],
            "blocked"
        );
    }
}
