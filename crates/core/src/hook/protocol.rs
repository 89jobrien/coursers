//! Protocol-aware hook output builders for Claude Code and Codex.

use crate::config::HookProtocol;
use serde_json::json;

/// Deny response + exit code. Claude = exit 2, Codex = exit 0.
pub fn deny_response(proto: HookProtocol, reason: &str) -> (String, i32) {
    let json = json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }
    });
    let exit_code = match proto {
        HookProtocol::Claude => 2,
        HookProtocol::Codex => 0,
    };
    (json.to_string(), exit_code)
}

/// Allow + rewrite response (PreToolUse).
pub fn rewrite_response(reason: &str, command: &str) -> String {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "permissionDecisionReason": reason,
            "updatedInput": { "command": command }
        }
    })
    .to_string()
}

/// Allow + tool swap response (PreToolUse).
pub fn tool_swap_response(tool_name: &str, tool_input: serde_json::Value) -> String {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "permissionDecisionReason":
                format!("crs tool-swap: Bash -> {tool_name}"),
            "updatedInput": {
                "tool_name": tool_name,
                "tool_input": tool_input,
            }
        }
    })
    .to_string()
}

/// System message response, valid for any hook event.
///
/// Uses the top-level `systemMessage` field rather than nesting under
/// `hookSpecificOutput`: the harness's `hookSpecificOutput.hookEventName` enum
/// does not include every event name (notably `PreCompact`/`PostCompact`), so
/// passing an arbitrary event string there fails schema validation and
/// silently discards the message. The top-level field has no such
/// restriction and works for every event, so `event` is unused here.
pub fn system_message_response(_event: &str, text: &str) -> String {
    json!({
        "systemMessage": text,
    })
    .to_string()
}

/// Filtered output response (PostToolUse).
///
/// No top-level `decision` field. Claude Code treats a top-level `decision` as the
/// *legacy* approve|block field and rejects the whole hook response when the value
/// is anything else — including `allow`:
///
///     Hook JSON output validation failed — decision: Invalid input (top-level
///     decision is the legacy approve|block field; for "allow" use
///     hookSpecificOutput.permissionDecision ...)
///
/// That error is non-fatal but it silently discards the redacted output and every
/// accumulated notification. Permission decisions belong in
/// `hookSpecificOutput.permissionDecision`, which this event does not set — it only
/// reshapes output the tool already produced.
pub fn filter_result_response(text: &str) -> String {
    json!({
        "type": "result",
        "message": text
    })
    .to_string()
}

/// Filtered output *and* system messages in one response (PostToolUse).
///
/// A redact rule and a `Notify` rule can both match the same tool call. Returning after
/// the redact response discarded the notifications, which made `Notify` rules
/// unreachable whenever a redact rule matched (on `Bash` the
/// `guardian/obfsck-redact-tool-output` rule always matches, so every notify arm on that
/// event was dead). Emitting both in a single response keeps the redacted output and the
/// notifications.
///
/// `messages` may be empty, in which case this is identical to
/// [`filter_result_response`].
pub fn filter_result_with_messages_response(text: &str, messages: &[String]) -> String {
    let joined = messages.join("\n");
    if joined.is_empty() {
        return filter_result_response(text);
    }
    json!({
        "type": "result",
        "message": text,
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": joined,
        }
    })
    .to_string()
}

/// Extract output text from tool_response, trying both field names.
/// Claude sends `output`, Codex sends `stdout`. If both present,
/// concatenates them (matching Codex hook convention).
pub fn extract_output(resp: &serde_json::Value) -> Option<String> {
    let output = resp.get("output").and_then(|v| v.as_str()).unwrap_or("");
    let stdout = resp.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
    let combined = format!("{stdout}{output}");
    if combined.is_empty() {
        None
    } else {
        Some(combined)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deny_claude_exits_2() {
        let (json_str, code) = deny_response(HookProtocol::Claude, "blocked");
        assert_eq!(code, 2);
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
    }

    #[test]
    fn deny_codex_exits_0() {
        let (json_str, code) = deny_response(HookProtocol::Codex, "blocked");
        assert_eq!(code, 0);
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
    }

    #[test]
    fn rewrite_response_has_updated_input() {
        let json_str = rewrite_response("crs rewrite: nu -c ls", "nu -c ls");
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(
            v["hookSpecificOutput"]["updatedInput"]["command"],
            "nu -c ls"
        );
    }

    /// Regression: `PreCompact`/`PostCompact` are not valid `hookSpecificOutput.hookEventName`
    /// values in the harness schema, so the message must ride the top-level field instead.
    #[test]
    fn system_message_response_uses_top_level_field() {
        for event in ["PreCompact", "PostCompact", "PostToolUse"] {
            let json_str = system_message_response(event, "hello");
            let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
            assert_eq!(v["systemMessage"], "hello");
            assert!(v.get("hookSpecificOutput").is_none());
        }
    }

    #[test]
    fn extract_output_prefers_combined() {
        let v = json!({"output": "hello", "stdout": "world"});
        assert_eq!(extract_output(&v).unwrap(), "worldhello");
    }

    #[test]
    fn extract_output_stdout_only() {
        let v = json!({"stdout": "codex"});
        assert_eq!(extract_output(&v).unwrap(), "codex");
    }

    #[test]
    fn extract_output_output_only() {
        let v = json!({"output": "claude"});
        assert_eq!(extract_output(&v).unwrap(), "claude");
    }

    #[test]
    fn extract_output_neither() {
        let v = json!({"exit_code": 0});
        assert!(extract_output(&v).is_none());
    }

    #[test]
    fn filter_with_messages_emits_both_redaction_and_notifications() {
        let msgs = vec!["cargo check failed".to_string(), "run fmt".to_string()];
        let json_str = filter_result_with_messages_response("REDACTED", &msgs);
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(v["message"], "REDACTED");
        assert_eq!(
            v["hookSpecificOutput"]["additionalContext"],
            "cargo check failed\nrun fmt"
        );
    }

    /// Regression: a top-level `decision` is the legacy approve|block field. Claude Code
    /// rejects the entire PostToolUse response when it carries `decision: "allow"`,
    /// which discards both the redaction and the notifications.
    #[test]
    fn filter_responses_omit_legacy_top_level_decision() {
        let with_messages = filter_result_with_messages_response(
            "REDACTED",
            &["cargo gate FAILED (exit 101)".to_string()],
        );
        for json_str in [filter_result_response("REDACTED"), with_messages] {
            let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
            assert!(
                v.get("decision").is_none(),
                "PostToolUse response must not carry a top-level `decision`: {json_str}"
            );
        }
    }

    #[test]
    fn filter_with_no_messages_matches_plain_filter_response() {
        let json_str = filter_result_with_messages_response("out", &[]);
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(
            v,
            serde_json::from_str::<serde_json::Value>(&filter_result_response("out")).unwrap()
        );
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Invariant: extract_output concatenates stdout before output.
        #[test]
        fn extract_output_concatenation_order(
            stdout in ".*",
            output in ".*",
        ) {
            let v = serde_json::json!({
                "stdout": stdout,
                "output": output,
            });
            let result = extract_output(&v);
            if stdout.is_empty() && output.is_empty() {
                prop_assert!(result.is_none());
            } else {
                let combined = result.unwrap();
                prop_assert_eq!(combined, format!("{stdout}{output}"));
            }
        }

        /// Invariant: extract_output returns None iff both fields are absent or empty.
        #[test]
        fn extract_output_none_iff_both_empty(
            has_stdout in proptest::bool::ANY,
            has_output in proptest::bool::ANY,
            stdout_val in ".*",
            output_val in ".*",
        ) {
            let mut map = serde_json::Map::new();
            if has_stdout {
                map.insert("stdout".into(), serde_json::Value::String(stdout_val.clone()));
            }
            if has_output {
                map.insert("output".into(), serde_json::Value::String(output_val.clone()));
            }
            let v = serde_json::Value::Object(map);
            let result = extract_output(&v);

            let effective_stdout = if has_stdout { &stdout_val } else { "" };
            let effective_output = if has_output { &output_val } else { "" };

            if effective_stdout.is_empty() && effective_output.is_empty() {
                prop_assert!(result.is_none());
            } else {
                prop_assert_eq!(
                    result.unwrap(),
                    format!("{effective_stdout}{effective_output}"),
                );
            }
        }

        /// Invariant: non-string JSON values in stdout/output are treated as absent
        /// (extract_output only reads string-typed fields via as_str()).
        #[test]
        fn extract_output_ignores_non_string_fields(
            stdout_num in proptest::num::i64::ANY,
            output_flag in proptest::bool::ANY,
        ) {
            let v = serde_json::json!({
                "stdout": stdout_num,
                "output": output_flag,
            });
            prop_assert!(extract_output(&v).is_none());
        }
    }
}
