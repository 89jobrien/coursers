use crs_core::history::{CommandRecord, CommandSource};
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use walkdir::WalkDir;

pub struct JsonlCommandSource {
    root: PathBuf,
    all_projects: bool,
    current_dir: Option<PathBuf>,
}

impl JsonlCommandSource {
    pub fn new(root: PathBuf, all_projects: bool, current_dir: Option<PathBuf>) -> Self {
        Self { root, all_projects, current_dir }
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
            .flat_map(move |entry| {
                let path = entry.into_path();
                let file = match std::fs::File::open(&path) {
                    Ok(f) => f,
                    Err(_) => return vec![],
                };
                let reader = BufReader::new(file);
                let all = all_projects;
                let cwd_filter = current_dir.clone();

                reader
                    .lines()
                    .filter_map(|l| l.ok())
                    .filter_map(move |line| {
                        let v: Value = serde_json::from_str(&line).ok()?;
                        if v.get("type")?.as_str()? != "assistant" {
                            return None;
                        }
                        let cwd = v.get("cwd")?.as_str().unwrap_or("").to_string();
                        let session_id = v.get("sessionId")
                            .or_else(|| v.get("session_id"))
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        let timestamp = v.get("timestamp")
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string());

                        if !all {
                            if let Some(ref cd) = cwd_filter {
                                if cwd != cd.to_string_lossy().as_ref() {
                                    return None;
                                }
                            }
                        }

                        let content = v.get("message")?.get("content")?.as_array()?;
                        let commands: Vec<CommandRecord> = content
                            .iter()
                            .filter_map(|block| {
                                if block.get("type")?.as_str()? != "tool_use" { return None; }
                                if block.get("name")?.as_str()? != "Bash" { return None; }
                                let command = block.get("input")?.get("command")?.as_str()?.to_string();
                                Some(CommandRecord {
                                    command,
                                    session_id: session_id.clone(),
                                    cwd: cwd.clone(),
                                    timestamp: timestamp.clone(),
                                })
                            })
                            .collect();
                        Some(commands)
                    })
                    .flatten()
                    .collect::<Vec<_>>()
            })
    }
}
