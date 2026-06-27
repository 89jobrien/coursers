//! Cumulative block statistics — records live interceptions by rule id.
//!
//! Stats file location (first found wins on read; write goes to first writable):
//!   1. `.ctx/crs-stats.json`    (project-local)
//!   2. `~/.config/coursers/crs-stats.json`  (global fallback)

use crate::error::CourserError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Cumulative block statistics: counts and timestamps per rule id.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Stats {
    /// rule_id → cumulative block count
    pub blocks: HashMap<String, u64>,
    /// rule_id → Unix timestamp of most recent block
    pub last_seen: HashMap<String, f64>,
}

/// Port for loading and saving block statistics.
pub trait StatsStore {
    fn load(&self) -> Result<Stats, CourserError>;
    fn save(&self, stats: &Stats) -> Result<(), CourserError>;

    /// Increment the block counter for `rule_id` and persist.
    fn record_block(&self, rule_id: &str) -> Result<(), CourserError> {
        let mut stats = self.load().unwrap_or_default();
        *stats.blocks.entry(rule_id.to_string()).or_insert(0) += 1;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        stats.last_seen.insert(rule_id.to_string(), now);
        self.save(&stats)
    }
}

/// Reads/writes stats JSON to a real file path.
pub struct FsStatsStore {
    pub path: PathBuf,
}

impl StatsStore for FsStatsStore {
    fn load(&self) -> Result<Stats, CourserError> {
        match std::fs::read_to_string(&self.path) {
            Ok(s) => serde_json::from_str(&s).map_err(CourserError::Json),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Stats::default()),
            Err(e) => Err(CourserError::Io(e)),
        }
    }

    fn save(&self, stats: &Stats) -> Result<(), CourserError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(CourserError::Io)?;
        }
        let json = serde_json::to_string_pretty(stats).map_err(CourserError::Json)?;
        std::fs::write(&self.path, json).map_err(CourserError::Io)
    }
}

/// In-memory store for tests. No filesystem I/O.
#[cfg(any(test, feature = "testing"))]
pub struct InMemoryStatsStore {
    inner: std::cell::RefCell<Stats>,
}

#[cfg(any(test, feature = "testing"))]
impl InMemoryStatsStore {
    pub fn new() -> Self {
        Self {
            inner: std::cell::RefCell::new(Stats::default()),
        }
    }

    pub fn get_stats(&self) -> Stats {
        self.inner.borrow().clone()
    }
}

#[cfg(any(test, feature = "testing"))]
impl Default for InMemoryStatsStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "testing"))]
impl StatsStore for InMemoryStatsStore {
    fn load(&self) -> Result<Stats, CourserError> {
        Ok(self.inner.borrow().clone())
    }

    fn save(&self, stats: &Stats) -> Result<(), CourserError> {
        *self.inner.borrow_mut() = stats.clone();
        Ok(())
    }
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

/// Load stats from a JSON file, returning defaults on missing or malformed file.
pub fn load(path: &std::path::Path) -> Stats {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Serialize stats to a JSON file, creating parent directories if needed.
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

    // ── StatsStore trait tests ──────────────────────────────────────────

    #[test]
    fn fs_stats_store_roundtrip() {
        let dir = TempDir::new().unwrap();
        let store = FsStatsStore {
            path: tmp_path(&dir),
        };
        store.record_block("no-grep").unwrap();
        store.record_block("no-grep").unwrap();
        store.record_block("no-cat").unwrap();
        let stats = store.load().unwrap();
        assert_eq!(stats.blocks["no-grep"], 2);
        assert_eq!(stats.blocks["no-cat"], 1);
    }

    #[test]
    fn fs_stats_store_missing_file_returns_default() {
        let store = FsStatsStore {
            path: PathBuf::from("/nonexistent/stats.json"),
        };
        let stats = store.load().unwrap();
        assert!(stats.blocks.is_empty());
    }

    #[test]
    fn in_memory_stats_store_roundtrip() {
        let store = InMemoryStatsStore::new();
        store.record_block("no-grep").unwrap();
        store.record_block("no-grep").unwrap();
        let stats = store.get_stats();
        assert_eq!(stats.blocks["no-grep"], 2);
    }
}
