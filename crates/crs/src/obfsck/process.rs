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
        if !out.status.success() && out.stdout.is_empty() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&out.stdout);
        // obfsck-mcp emits one JSON object per line; take the last non-empty line.
        stdout
            .lines()
            .filter(|l| !l.trim().is_empty())
            .last()
            .and_then(|l| serde_json::from_str(l).ok())
    }
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

        let resp = match self.call(&req) {
            Some(v) => v,
            None => return vec![],
        };

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

        let resp = match self.call(&req) {
            Some(v) => v,
            None => return vec![],
        };

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
}
