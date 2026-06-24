//! Cumulative block statistics — records live interceptions by rule id.
//!
//! Stats file location (first found wins on read; write goes to first writable):
//!   1. `.ctx/crs-stats.json`    (project-local)
//!   2. `~/.config/coursers/crs-stats.json`  (global fallback)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Stats {
    /// rule_id → cumulative block count
    pub blocks: HashMap<String, u64>,
    /// rule_id → Unix timestamp of most recent block
    pub last_seen: HashMap<String, f64>,
}

/// Resolve the stats file path. Project-local `.ctx/crs-stats.json` wins over global.
pub fn stats_path() -> PathBuf {
    let local = PathBuf::from(".ctx/crs-stats.json");
    if local.parent().map(|p| p.exists()).unwrap_or(false) {
        return local;
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".config/coursers/crs-stats.json")
}

pub fn load(path: &std::path::Path) -> Stats {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(path: &std::path::Path, stats: &Stats) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(stats) {
        let _ = std::fs::write(path, json);
    }
}

/// Increment the block counter for `rule_id` at the given path.
pub fn record_block(path: &std::path::Path, rule_id: &str) {
    let mut stats = load(path);
    *stats.blocks.entry(rule_id.to_string()).or_insert(0) += 1;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    stats.last_seen.insert(rule_id.to_string(), now);
    save(path, &stats);
}

/// Return block counts sorted by count descending.
pub fn sorted_blocks(stats: &Stats) -> Vec<(String, u64)> {
    let mut v: Vec<(String, u64)> = stats.blocks.iter().map(|(k, v)| (k.clone(), *v)).collect();
    v.sort_by_key(|b| std::cmp::Reverse(b.1));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp_path(dir: &TempDir) -> PathBuf {
        dir.path().join("crs-stats.json")
    }

    #[test]
    fn record_block_increments_count() {
        let dir = TempDir::new().unwrap();
        let path = tmp_path(&dir);
        record_block(&path, "no-grep-use-tool");
        record_block(&path, "no-grep-use-tool");
        record_block(&path, "no-ls-use-glob");
        let stats = load(&path);
        assert_eq!(stats.blocks["no-grep-use-tool"], 2);
        assert_eq!(stats.blocks["no-ls-use-glob"], 1);
    }

    #[test]
    fn sorted_blocks_orders_by_count_desc() {
        let dir = TempDir::new().unwrap();
        let path = tmp_path(&dir);
        record_block(&path, "a");
        record_block(&path, "b");
        record_block(&path, "b");
        record_block(&path, "b");
        record_block(&path, "a");
        let stats = load(&path);
        let sorted = sorted_blocks(&stats);
        assert_eq!(sorted[0], ("b".to_string(), 3));
        assert_eq!(sorted[1], ("a".to_string(), 2));
    }

    #[test]
    fn missing_file_returns_default() {
        let stats = load(std::path::Path::new("/nonexistent/path.json"));
        assert!(stats.blocks.is_empty());
    }

    #[test]
    fn last_seen_updated_on_record() {
        let dir = TempDir::new().unwrap();
        let path = tmp_path(&dir);
        record_block(&path, "no-cat-use-read");
        let stats = load(&path);
        assert!(stats.last_seen.contains_key("no-cat-use-read"));
        assert!(*stats.last_seen.get("no-cat-use-read").unwrap() > 0.0);
    }
}
