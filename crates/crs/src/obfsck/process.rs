/// ProcessObfsckMcpClient — communicates with `obfsck-mcp` via JSON-RPC stdio.
///
/// Spawns the binary once per call (stateless; acceptable for infrequent use).
/// All methods fail-open: if the binary is missing or returns an error, returns empty.
use std::io::Write as _;
use std::process::{Command, Stdio};

use crs_core::obfsck::{AuditHit, FilterSuggestion, ObfsckMcp};

pub struct ProcessObfsckMcpClient;

impl ProcessObfsckMcpClient {
    /// Send one JSON-RPC request and return the parsed response value.
    fn call(&self, request: &serde_json::Value) -> Option<serde_json::Value> {
        let mut child = Command::new("obfsck-mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let req_str = serde_json::to_string(request).ok()?;

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(req_str.as_bytes());
            let _ = stdin.write_all(b"\n");
        }

        let out = child.wait_with_output().ok()?;
        parse_jsonrpc_response(out.status.success(), &out.stdout)
    }
}

// ---------------------------------------------------------------------------
// Pure predicates — no I/O, fully unit-testable
// ---------------------------------------------------------------------------

/// Extracts the last non-empty JSON line from a process stdout buffer.
///
/// Returns `None` when the process failed without producing any output, or
/// when there are no parseable JSON lines.
pub(crate) fn parse_jsonrpc_response(
    status_success: bool,
    stdout: &[u8],
) -> Option<serde_json::Value> {
    if !status_success && stdout.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(stdout);
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .next_back()
        .and_then(|l| serde_json::from_str(l).ok())
}

/// Parses an `audit` JSON-RPC response into `AuditHit`s.
pub(crate) fn parse_audit_hits(resp: &serde_json::Value) -> Vec<AuditHit> {
    resp["result"]["hits"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|h| {
                    let label = h["label"].as_str()?.to_owned();
                    let count = h["count"].as_u64().unwrap_or(0) as usize;
                    Some(AuditHit { label, count })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parses a `generate-filters` JSON-RPC response into `FilterSuggestion`s.
pub(crate) fn parse_filter_suggestions(resp: &serde_json::Value) -> Vec<FilterSuggestion> {
    resp["result"]["suggestions"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|s| {
                    let pattern = s["pattern"].as_str()?.to_owned();
                    let label = s["label"].as_str()?.to_owned();
                    Some(FilterSuggestion { pattern, label })
                })
                .collect()
        })
        .unwrap_or_default()
}

impl ObfsckMcp for ProcessObfsckMcpClient {
    fn audit(&self, text: &str) -> Vec<AuditHit> {
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "audit",
                "arguments": { "text": text }
            }
        });

        match self.call(&req) {
            Some(resp) => parse_audit_hits(&resp),
            None => vec![],
        }
    }

    fn generate_filters(&self, examples: &[String]) -> Vec<FilterSuggestion> {
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "generate-filters",
                "arguments": { "examples": examples }
            }
        });

        match self.call(&req) {
            Some(resp) => parse_filter_suggestions(&resp),
            None => vec![],
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests for pure predicate functions
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_jsonrpc_response_returns_none_on_failure_with_no_stdout() {
        assert!(parse_jsonrpc_response(false, b"").is_none());
    }

    #[test]
    fn parse_jsonrpc_response_returns_last_json_line() {
        let stdout = b"ignored\n{\"result\":\"ok\"}\n";
        let val = parse_jsonrpc_response(true, stdout).unwrap();
        assert_eq!(val["result"], "ok");
    }

    #[test]
    fn parse_jsonrpc_response_accepts_failed_status_with_stdout() {
        // Some versions emit output even on non-zero exit; we should still parse.
        let stdout = b"{\"result\":\"ok\"}\n";
        let val = parse_jsonrpc_response(false, stdout).unwrap();
        assert_eq!(val["result"], "ok");
    }

    #[test]
    fn parse_audit_hits_extracts_hits() {
        let resp = serde_json::json!({
            "result": {
                "hits": [
                    { "label": "secret", "count": 3 },
                    { "label": "token", "count": 1 }
                ]
            }
        });
        let hits = parse_audit_hits(&resp);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].label, "secret");
        assert_eq!(hits[0].count, 3);
    }

    #[test]
    fn parse_audit_hits_returns_empty_on_missing_field() {
        let resp = serde_json::json!({ "result": {} });
        assert!(parse_audit_hits(&resp).is_empty());
    }

    #[test]
    fn parse_filter_suggestions_extracts_suggestions() {
        let resp = serde_json::json!({
            "result": {
                "suggestions": [
                    { "pattern": "foo.*", "label": "foo pattern" }
                ]
            }
        });
        let suggestions = parse_filter_suggestions(&resp);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].pattern, "foo.*");
        assert_eq!(suggestions[0].label, "foo pattern");
    }

    #[test]
    fn parse_filter_suggestions_returns_empty_on_missing_field() {
        let resp = serde_json::json!({ "result": {} });
        assert!(parse_filter_suggestions(&resp).is_empty());
    }
}
