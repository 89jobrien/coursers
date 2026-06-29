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
