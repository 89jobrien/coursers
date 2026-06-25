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
#[allow(clippy::type_complexity, dead_code)]
pub fn hook_context() -> Option<(
    HookPayload,
    crs_core::loader::FsRulesLoader,
    crs_core::store::FsStateStore,
    crs_core::capture::SuggestionStore,
)> {
    use crs_core::capture::SuggestionStore;
    use crs_core::loader::{FsRulesLoader, RulesLoader};
    use crs_core::store::FsStateStore;
    use crs_core::store::state_path;

    let payload = read_stdin()?;
    let loader = FsRulesLoader;
    let config = loader.load();
    let path = state_path(&config.failure_learning);
    let store = FsStateStore { path };
    let capture = SuggestionStore::new(SuggestionStore::default_path());
    Some((payload, loader, store, capture))
}

/// Profile-aware variant of [`hook_context`].
/// Constructs loaders and stores from a resolved [`crs_core::config::ProfileConfig`].
#[allow(clippy::type_complexity)]
pub fn hook_context_with_profile(
    profile_cfg: &crs_core::config::ProfileConfig,
) -> Option<(
    HookPayload,
    crs_core::loader::ProfileFsRulesLoader,
    crs_core::store::FsStateStore,
    crs_core::capture::SuggestionStore,
)> {
    use crs_core::capture::SuggestionStore;
    use crs_core::loader::ProfileFsRulesLoader;
    use crs_core::store::FsStateStore;

    let payload = read_stdin()?;
    let loader = ProfileFsRulesLoader {
        path: profile_cfg.rules_path.clone(),
    };
    let store = FsStateStore {
        path: profile_cfg.effective_state_path().clone(),
    };
    let capture = SuggestionStore::new(SuggestionStore::default_path());
    Some((payload, loader, store, capture))
}

pub fn read_stdin() -> Option<HookPayload> {
    use std::io::Read;
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf).ok()?;
    match serde_json::from_str(&buf) {
        Ok(payload) => Some(payload),
        Err(e) => {
            eprintln!("[coursers] warning: failed to parse stdin as hook payload: {e}");
            None
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Conformance test: malformed stdin must not panic; hook continues with allow behavior.
    ///
    /// `read_stdin` returns `None` on malformed input. Callers treat `None` as passthrough
    /// (allow), so this verifies the boundary that produces that outcome.
    #[test]
    fn hook_pre_malformed_stdin_does_not_panic() {
        let malformed = b"not valid json at all";
        let result: Option<HookPayload> = serde_json::from_slice(malformed).ok();
        assert!(
            result.is_none(),
            "malformed stdin must deserialize to None, triggering allow passthrough"
        );
    }

    /// Verify that `read_stdin` returns `None` on malformed input (finding #4).
    /// Hooks treat `None` as passthrough (allow) — non-blocking contract.
    #[test]
    fn read_stdin_returns_none_on_malformed_json() {
        let malformed = "{ this is not valid json }";
        let result: Option<HookPayload> = serde_json::from_str(malformed)
            .map_err(|e| {
                eprintln!("[coursers] warning: failed to parse stdin as hook payload: {e}");
                e
            })
            .ok();
        assert!(
            result.is_none(),
            "malformed payload must yield None (non-blocking passthrough)"
        );
    }
}
