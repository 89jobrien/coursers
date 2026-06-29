use coursers_core::history::{CommandRecord, CommandSource};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use walkdir::WalkDir;

/// [`CommandSource`] adapter that walks a directory tree and parses `.jsonl` session files.
pub struct JsonlCommandSource {
    root: PathBuf,
    all_projects: bool,
    current_dir: Option<PathBuf>,
}

impl JsonlCommandSource {
    /// Create a new source rooted at `root`. Pass `all_projects: true` to skip cwd filtering.
    pub fn new(root: PathBuf, all_projects: bool, current_dir: Option<PathBuf>) -> Self {
        Self {
            root,
            all_projects,
            current_dir,
        }
    }
}

impl CommandSource for JsonlCommandSource {
    fn commands(&self) -> impl Iterator<Item = CommandRecord> {
        let root = self.root.clone();
        let all_projects = self.all_projects;
        let current_dir = self.current_dir.clone();

        WalkDir::new(&root)
            .min_depth(1)
            .max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_type().is_file()
                    && e.path().extension().map(|x| x == "jsonl").unwrap_or(false)
            })
            .flat_map(move |entry| parse_file(entry.into_path(), all_projects, current_dir.clone()))
    }
}

/// Parse JSONL session content from a string (used by fuzz targets and tests).
///
/// This is the pure string-based entry point — `parse_file` is the I/O wrapper.
pub fn parse_session_content(content: &str) -> Vec<CommandRecord> {
    let mut bash_calls: HashMap<String, (String, String, String, Option<String>)> = HashMap::new();
    let mut output_sizes: HashMap<String, usize> = HashMap::new();

    for line in content.lines() {
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match v.get("type").and_then(|t| t.as_str()) {
            Some("assistant") => {
                parse_assistant_block(&v, true, &None, &mut bash_calls);
            }
            Some("user") => {
                parse_user_block(&v, &mut output_sizes);
            }
            _ => {}
        }
    }

    bash_calls
        .into_iter()
        .map(
            |(id, (command, cwd, session_id, timestamp))| CommandRecord {
                command,
                session_id,
                cwd,
                timestamp,
                output_bytes: output_sizes.get(&id).copied(),
            },
        )
        .collect()
}

/// Parse a single JSONL session file.
///
/// Two-pass over the lines:
// TODO(#41): pick one arrow style (-> vs the unicode arrow used elsewhere)
///   Pass 1 -- collect assistant tool_use records: tool_use_id -> (command, cwd, session_id, ts)
///   Pass 2 -- collect user tool_result records:   tool_use_id -> output_bytes
///
/// Then join on tool_use_id to produce CommandRecord values with real output_bytes.
fn parse_file(
    path: PathBuf,
    all_projects: bool,
    current_dir: Option<PathBuf>,
) -> Vec<CommandRecord> {
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    // tool_use_id -> (command, cwd, session_id, timestamp)
    let mut bash_calls: HashMap<String, (String, String, String, Option<String>)> = HashMap::new();
    // tool_use_id -> output byte count
    let mut output_sizes: HashMap<String, usize> = HashMap::new();

    for line in content.lines() {
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        match v.get("type").and_then(|t| t.as_str()) {
            Some("assistant") => {
                parse_assistant_block(&v, all_projects, &current_dir, &mut bash_calls);
            }
            Some("user") => {
                parse_user_block(&v, &mut output_sizes);
            }
            _ => {}
        }
    }

    bash_calls
        .into_iter()
        .map(
            |(id, (command, cwd, session_id, timestamp))| CommandRecord {
                command,
                session_id,
                cwd,
                timestamp,
                output_bytes: output_sizes.get(&id).copied(),
            },
        )
        .collect()
}

/// Extract Bash tool_use commands from an assistant message block.
fn parse_assistant_block(
    v: &Value,
    all_projects: bool,
    current_dir: &Option<PathBuf>,
    bash_calls: &mut HashMap<String, (String, String, String, Option<String>)>,
) {
    let cwd = v
        .get("cwd")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    if !all_projects
        && let Some(cd) = current_dir
        && cwd != cd.to_string_lossy().as_ref()
    {
        return;
    }
    let session_id = v
        .get("sessionId")
        .or_else(|| v.get("session_id"))
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    let timestamp = v
        .get("timestamp")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());

    let Some(content_arr) = v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    else {
        return;
    };

    for block in content_arr {
        if block.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
            continue;
        }
        if block.get("name").and_then(|n| n.as_str()) != Some("Bash") {
            continue;
        }
        let Some(cmd) = block
            .get("input")
            .and_then(|i| i.get("command"))
            .and_then(|c| c.as_str())
        else {
            continue;
        };
        // Prefer the tool_use id for output-size correlation.
        // If absent (e.g. test fixtures), generate a unique key so commands
        // are still collected but won't match any tool_result.
        let id = block
            .get("id")
            .and_then(|i| i.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("__no_id_{}_{}", cmd.len(), bash_calls.len()));
        bash_calls.insert(
            id,
            (
                cmd.to_string(),
                cwd.clone(),
                session_id.clone(),
                timestamp.clone(),
            ),
        );
    }
}

/// Extract output byte counts from user tool_result blocks.
fn parse_user_block(v: &Value, output_sizes: &mut HashMap<String, usize>) {
    let Some(content_arr) = v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    else {
        return;
    };

    for block in content_arr {
        if block.get("type").and_then(|t| t.as_str()) != Some("tool_result") {
            continue;
        }
        let Some(id) = block.get("tool_use_id").and_then(|i| i.as_str()) else {
            continue;
        };
        // content may be a string or an array of content blocks
        let byte_len = match block.get("content") {
            Some(Value::String(s)) => s.len(),
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|item| item.get("text").and_then(|t| t.as_str()).map(|s| s.len()))
                .sum(),
            _ => 0,
        };
        if byte_len > 0 {
            output_sizes.insert(id.to_string(), byte_len);
        }
    }
}
