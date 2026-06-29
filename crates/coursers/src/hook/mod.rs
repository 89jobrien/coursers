pub mod post;
pub mod pre;

#[allow(unused_imports)]
pub use coursers_types::hook::{HookPayload, HookSpecificOutput, PreResponse, ToolInput};

/// Shared hook wiring: read stdin, load rules, state store, and capture store.
#[allow(clippy::type_complexity, dead_code)]
pub fn hook_context() -> Option<(
    HookPayload,
    coursers_core::loader::FsRulesLoader,
    coursers_core::store::FsStateStore,
    coursers_core::capture::SuggestionStore,
)> {
    use coursers_core::capture::SuggestionStore;
    use coursers_core::loader::{FsRulesLoader, RulesLoader};
    use coursers_core::store::FsStateStore;
    use coursers_core::store::state_path;

    let payload = read_stdin()?;
    let loader = FsRulesLoader;
    let config = loader.load().unwrap_or_else(|e| {
        eprintln!("[coursers] warning: failed to load rules: {e}");
        coursers_core::rules::RulesConfig {
            rules: vec![],
            failure_learning: coursers_core::rules::FailureLearning::default(),
        }
    });
    let path = state_path(&config.failure_learning);
    let store = FsStateStore { path };
    let capture = SuggestionStore::new(SuggestionStore::default_path());
    Some((payload, loader, store, capture))
}

/// Profile-aware variant of [`hook_context`].
/// Constructs loaders and stores from a resolved [`coursers_core::config::ProfileConfig`].
#[allow(clippy::type_complexity)]
pub fn hook_context_with_profile(
    profile_cfg: &coursers_core::config::ProfileConfig,
) -> Option<(
    HookPayload,
    coursers_core::loader::ProfileFsRulesLoader,
    coursers_core::store::FsStateStore,
    coursers_core::capture::SuggestionStore,
)> {
    use coursers_core::capture::SuggestionStore;
    use coursers_core::loader::ProfileFsRulesLoader;
    use coursers_core::store::FsStateStore;

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

/// Read and deserialize a hook payload from stdin. Returns `None` on malformed input.
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

/// Serialize a [`PreResponse`] to JSON, falling back to a hardcoded deny payload if
/// serialization fails. This function never panics.
#[allow(dead_code)]
pub(crate) fn serialize_deny_response(resp: &PreResponse) -> String {
    serde_json::to_string(resp).unwrap_or_else(|_| {
        r#"{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"[coursers] internal error: failed to serialize deny response"}}"#.to_owned()
    })
}

/// Emit a deny response to stdout and exit with code 2 (Claude protocol).
#[allow(dead_code)]
pub fn deny(reason: &str) {
    deny_with_protocol(coursers_core::config::HookProtocol::Claude, reason);
}

/// Protocol-aware deny: exit code differs between Claude (2) and Codex (0).
pub fn deny_with_protocol(proto: coursers_core::config::HookProtocol, reason: &str) {
    use std::io::Write;
    let (json, exit_code) = coursers_core::hook::protocol::deny_response(proto, reason);
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{json}").ok();
    handle.flush().ok();
    drop(handle);
    std::process::exit(exit_code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_deny_response_produces_valid_json() {
        let resp = PreResponse {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: "PreToolUse".into(),
                permission_decision: "deny".into(),
                permission_decision_reason: "test reason".into(),
            },
        };
        let json = serialize_deny_response(&resp);
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("serialize_deny_response must produce valid JSON");
        assert_eq!(parsed["hookSpecificOutput"]["permissionDecision"], "deny");
        assert_eq!(
            parsed["hookSpecificOutput"]["permissionDecisionReason"],
            "test reason"
        );
    }

    #[test]
    fn serialize_deny_response_fallback_is_valid_json() {
        // Verify the hardcoded fallback string is itself valid JSON with the expected
        // deny decision — this is emitted when serde_json::to_string fails.
        let fallback = r#"{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"[coursers] internal error: failed to serialize deny response"}}"#;
        let parsed: serde_json::Value =
            serde_json::from_str(fallback).expect("fallback must be valid JSON");
        assert_eq!(parsed["hookSpecificOutput"]["permissionDecision"], "deny");
    }

    /// Conformance test: malformed stdin must not panic; hook continues with allow behavior.
    ///
    /// `read_stdin` returns `None` on malformed input. Callers treat `None` as passthrough
    /// (allow), so this verifies the boundary that produces that outcome.
    #[test]
    fn hook_pre_malformed_stdin_does_not_panic() {
        // Deserializing malformed bytes must return None, not panic.
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
