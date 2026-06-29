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

/// System message response (PostToolUse or other events).
pub fn system_message_response(event: &str, text: &str) -> String {
    json!({
        "hookSpecificOutput": {
            "hookEventName": event,
            "permissionDecision": "allow",
            "systemMessage": text,
        }
    })
    .to_string()
}

/// Filtered output response (PostToolUse).
pub fn filter_result_response(text: &str) -> String {
    json!({
        "type": "result",
        "message": text,
        "decision": "allow"
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
    }
}
