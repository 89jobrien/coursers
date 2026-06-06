//! Hook execution log backed by redb.
//!
//! Records every hook pipeline evaluation: what event fired, which rules matched,
//! what action was taken, and when. Queryable by time range, event type, rule label,
//! or action kind.

use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Table: timestamp_ns (u64) -> JSON-encoded LogEntry
const LOG_TABLE: TableDefinition<u64, &str> = TableDefinition::new("hook_log");

/// A single hook execution log entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogEntry {
    pub timestamp: u64,
    pub event: String,
    pub tool_name: Option<String>,
    pub target: Option<String>,
    pub exit_code: Option<i64>,
    /// Rules that matched (by label or index).
    pub matched_rules: Vec<String>,
    /// Final outcome.
    pub outcome: Outcome,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Pass,
    Deny { message: String },
    Rewrite { to: String },
    SideEffect { commands_run: usize },
    Notify { count: usize },
}

/// Default database path.
pub fn db_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/crs/hook-log.redb")
}

/// Open (or create) the log database.
pub fn open_db(path: &std::path::Path) -> Result<Database, redb::DatabaseError> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    Database::create(path)
}

/// Current timestamp in nanoseconds since epoch.
fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

/// Write a log entry. Non-blocking — silently drops on error.
pub fn record(db: &Database, entry: &LogEntry) {
    let Ok(txn) = db.begin_write() else { return };
    {
        let Ok(mut table) = txn.open_table(LOG_TABLE) else {
            return;
        };
        let Ok(json) = serde_json::to_string(entry) else {
            return;
        };
        let _ = table.insert(entry.timestamp, json.as_str());
    }
    let _ = txn.commit();
}

/// Build a LogEntry from pipeline context and result.
pub fn entry_from_pipeline(
    ctx: &super::pipeline::HookContext,
    result: &super::pipeline::PipelineResult,
    matched_labels: Vec<String>,
) -> LogEntry {
    let outcome = if let Some(ref msg) = result.deny {
        Outcome::Deny {
            message: msg.clone(),
        }
    } else if let Some(ref to) = result.rewrite {
        Outcome::Rewrite { to: to.clone() }
    } else if !result.messages.is_empty() {
        Outcome::Notify {
            count: result.messages.len(),
        }
    } else {
        Outcome::Pass
    };

    LogEntry {
        timestamp: now_ns(),
        event: ctx
            .event
            .map(|e| format!("{e:?}"))
            .unwrap_or_else(|| "unknown".into()),
        tool_name: ctx.tool_name.clone(),
        target: ctx.target.clone(),
        exit_code: ctx.exit_code,
        matched_rules: matched_labels,
        outcome,
    }
}

// ---------------------------------------------------------------------------
// Query
// ---------------------------------------------------------------------------

/// Query options for reading log entries.
#[derive(Debug, Default)]
pub struct LogQuery {
    /// Only entries after this timestamp (ns).
    pub after: Option<u64>,
    /// Only entries before this timestamp (ns).
    pub before: Option<u64>,
    /// Filter by event name (case-insensitive contains).
    pub event: Option<String>,
    /// Filter by outcome kind.
    pub outcome_kind: Option<String>,
    /// Max entries to return.
    pub limit: usize,
}

/// Read log entries matching the query (newest first).
pub fn query(db: &Database, q: &LogQuery) -> Vec<LogEntry> {
    let Ok(txn) = db.begin_read() else {
        return vec![];
    };
    let Ok(table) = txn.open_table(LOG_TABLE) else {
        return vec![];
    };

    let limit = if q.limit == 0 { 100 } else { q.limit };
    let mut entries = Vec::with_capacity(limit);

    // Iterate in reverse (newest first).
    let iter = table.iter();
    let Ok(iter) = iter else { return vec![] };

    for item in iter.rev() {
        if entries.len() >= limit {
            break;
        }
        let Ok(kv) = item else { continue };
        let ts = kv.0.value();
        let json_str = kv.1.value();

        if let Some(after) = q.after
            && ts <= after
        {
            continue;
        }
        if let Some(before) = q.before
            && ts >= before
        {
            break; // sorted desc, so all remaining are older
        }

        let Ok(entry) = serde_json::from_str::<LogEntry>(json_str) else {
            continue;
        };

        if let Some(ref ev) = q.event
            && !entry.event.to_lowercase().contains(&ev.to_lowercase())
        {
            continue;
        }

        if let Some(ref kind) = q.outcome_kind {
            let entry_kind = match &entry.outcome {
                Outcome::Pass => "pass",
                Outcome::Deny { .. } => "deny",
                Outcome::Rewrite { .. } => "rewrite",
                Outcome::SideEffect { .. } => "side-effect",
                Outcome::Notify { .. } => "notify",
            };
            if entry_kind != kind.as_str() {
                continue;
            }
        }

        entries.push(entry);
    }

    entries
}

/// Count total entries in the log.
pub fn count(db: &Database) -> u64 {
    let Ok(txn) = db.begin_read() else { return 0 };
    let Ok(table) = txn.open_table(LOG_TABLE) else {
        return 0;
    };
    table.len().unwrap_or(0)
}

/// Prune entries older than `before_ns`.
pub fn prune(db: &Database, before_ns: u64) -> u64 {
    let Ok(txn) = db.begin_write() else { return 0 };
    let mut removed = 0u64;
    {
        let Ok(mut table) = txn.open_table(LOG_TABLE) else {
            return 0;
        };
        // Collect keys to remove.
        let keys: Vec<u64> = table
            .iter()
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|item| item.ok())
            .map(|kv| kv.0.value())
            .take_while(|ts| *ts < before_ns)
            .collect();
        for key in keys {
            if table.remove(key).is_ok() {
                removed += 1;
            }
        }
    }
    let _ = txn.commit();
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_db() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.redb");
        let db = Database::create(&path).unwrap();
        (dir, db)
    }

    #[test]
    fn record_and_query() {
        let (_dir, db) = tmp_db();
        let entry = LogEntry {
            timestamp: now_ns(),
            event: "PreToolUse".into(),
            tool_name: Some("Bash".into()),
            target: Some("grep foo".into()),
            exit_code: None,
            matched_rules: vec!["no-grep-use-tool".into()],
            outcome: Outcome::Deny {
                message: "blocked".into(),
            },
        };
        record(&db, &entry);

        let results = query(&db, &LogQuery::default());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event, "PreToolUse");
        assert!(matches!(results[0].outcome, Outcome::Deny { .. }));
    }

    #[test]
    fn query_filters_by_event() {
        let (_dir, db) = tmp_db();
        for event in ["PreToolUse", "PostToolUse", "Stop"] {
            let entry = LogEntry {
                timestamp: now_ns(),
                event: event.into(),
                tool_name: None,
                target: None,
                exit_code: None,
                matched_rules: vec![],
                outcome: Outcome::Pass,
            };
            record(&db, &entry);
        }

        let results = query(
            &db,
            &LogQuery {
                event: Some("Post".into()),
                ..Default::default()
            },
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event, "PostToolUse");
    }

    #[test]
    fn query_filters_by_outcome() {
        let (_dir, db) = tmp_db();
        record(
            &db,
            &LogEntry {
                timestamp: now_ns(),
                event: "PreToolUse".into(),
                tool_name: None,
                target: None,
                exit_code: None,
                matched_rules: vec![],
                outcome: Outcome::Pass,
            },
        );
        record(
            &db,
            &LogEntry {
                timestamp: now_ns() + 1,
                event: "PreToolUse".into(),
                tool_name: None,
                target: None,
                exit_code: None,
                matched_rules: vec!["rule1".into()],
                outcome: Outcome::Deny {
                    message: "no".into(),
                },
            },
        );

        let results = query(
            &db,
            &LogQuery {
                outcome_kind: Some("deny".into()),
                ..Default::default()
            },
        );
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn prune_removes_old_entries() {
        let (_dir, db) = tmp_db();
        let old = 1_000_000u64;
        let recent = now_ns();
        record(
            &db,
            &LogEntry {
                timestamp: old,
                event: "old".into(),
                tool_name: None,
                target: None,
                exit_code: None,
                matched_rules: vec![],
                outcome: Outcome::Pass,
            },
        );
        record(
            &db,
            &LogEntry {
                timestamp: recent,
                event: "recent".into(),
                tool_name: None,
                target: None,
                exit_code: None,
                matched_rules: vec![],
                outcome: Outcome::Pass,
            },
        );

        let removed = prune(&db, recent - 1);
        assert_eq!(removed, 1);
        assert_eq!(count(&db), 1);
    }

    #[test]
    fn count_works() {
        let (_dir, db) = tmp_db();
        assert_eq!(count(&db), 0);
        record(
            &db,
            &LogEntry {
                timestamp: now_ns(),
                event: "x".into(),
                tool_name: None,
                target: None,
                exit_code: None,
                matched_rules: vec![],
                outcome: Outcome::Pass,
            },
        );
        assert_eq!(count(&db), 1);
    }
}
