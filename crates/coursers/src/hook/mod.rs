pub mod post;
pub mod pre;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Full Claude Code hook payload (PreToolUse or PostToolUse)
#[derive(Debug, Deserialize)]
pub struct HookPayload {
    pub tool_name: Option<String>,
    pub tool_input: Option<ToolInput>,
    pub tool_response: Option<Value>,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ToolInput {
    pub command: Option<String>,
}

/// PreToolUse response envelope
#[derive(Debug, Serialize)]
pub struct PreResponse {
    #[serde(rename = "hookSpecificOutput")]
    pub hook_specific_output: HookSpecificOutput,
}

#[derive(Debug, Serialize)]
pub struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    pub hook_event_name: String,
    #[serde(rename = "permissionDecision")]
    pub permission_decision: String,
    #[serde(rename = "permissionDecisionReason")]
    pub permission_decision_reason: String,
}

/// Shared hook wiring: read stdin, load rules, state store, and capture store.
#[allow(clippy::type_complexity)]
pub fn hook_context() -> Option<(
    HookPayload,
    crs_core::loader::FsRulesLoader,
    crs_core::store::FsStateStore,
    crs_core::capture::SuggestionStore,
)> {
    use crs_core::capture::SuggestionStore;
    use crs_core::loader::{FsRulesLoader, RulesLoader};
    use crs_core::state::state_path;
    use crs_core::store::FsStateStore;

    let payload = read_stdin()?;
    let loader = FsRulesLoader;
    let config = loader.load();
    let path = state_path(&config.failure_learning);
    let store = FsStateStore { path };
    let capture = SuggestionStore::new(SuggestionStore::default_path());
    Some((payload, loader, store, capture))
}

pub fn read_stdin() -> Option<HookPayload> {
    use std::io::Read;
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf).ok()?;
    serde_json::from_str(&buf).ok()
}

pub fn deny(reason: &str) {
    use std::io::Write;
    let resp = PreResponse {
        hook_specific_output: HookSpecificOutput {
            hook_event_name: "PreToolUse".into(),
            permission_decision: "deny".into(),
            permission_decision_reason: reason.into(),
        },
    };
    let json = serde_json::to_string(&resp).unwrap();
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{}", json).ok();
    handle.flush().ok();
    drop(handle);
    std::process::exit(2);
}
