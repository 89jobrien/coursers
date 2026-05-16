//! Suggestion capture for fine-tuning dataset collection.
//!
//! When `coursers pre` blocks a command, it logs a `SuggestionRecord` pairing
//! the original command with the fast-alternative suggestion from the rule message.
//! When `coursers post` sees a successful command that matches a pending suggestion,
//! it marks the record accepted — providing a positive training signal.
//!
//! Records are stored as newline-delimited JSON in
//! `~/.config/coursers/suggestions.jsonl`. Dedup key is `(original, suggestion)`;
//! duplicates increment `count` and upgrade `accepted` from false → true.

use fd_lock::RwLock as FdRwLock;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuggestionRecord {
    /// ISO 8601 timestamp of the block event.
    pub ts: String,
    /// Exact command that was blocked. Part of dedup key.
    pub original: String,
    /// Fast-alternative suggestion from the rule message. Part of dedup key.
    pub suggestion: String,
    /// Rule id that fired (e.g. `no-grep-use-tool`).
    pub rule_id: String,
    /// Working directory at time of block.
    pub cwd: String,
    /// Git repo name derived from cwd (last path component).
    pub repo: Option<String>,
    /// Claude Code session id from the hook payload.
    pub session_id: Option<String>,
    /// Tool name (e.g. `Bash`).
    pub tool_name: String,
    /// True if the suggestion was subsequently used in the same session.
    pub accepted: bool,
    /// Timestamp of acceptance.
    pub accepted_ts: Option<String>,
    /// Exit code of the accepted command.
    pub exit_code: Option<i64>,
    /// Number of times this (original, suggestion) pair has been seen.
    pub count: u32,
}

impl SuggestionRecord {
    pub fn new(
        original: impl Into<String>,
        suggestion: impl Into<String>,
        rule_id: impl Into<String>,
        cwd: impl Into<String>,
        session_id: Option<String>,
        tool_name: impl Into<String>,
    ) -> Self {
        let cwd = cwd.into();
        let repo = repo_from_cwd(&cwd);
        Self {
            ts: now_iso8601(),
            original: original.into(),
            suggestion: suggestion.into(),
            rule_id: rule_id.into(),
            cwd,
            repo,
            session_id,
            tool_name: tool_name.into(),
            accepted: false,
            accepted_ts: None,
            exit_code: None,
            count: 1,
        }
    }
}

/// Dedup key: normalized (trimmed) original + suggestion.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DedupeKey {
    pub original: String,
    pub suggestion: String,
}

impl DedupeKey {
    pub fn from_record(r: &SuggestionRecord) -> Self {
        Self {
            original: r.original.trim().to_string(),
            suggestion: r.suggestion.trim().to_string(),
        }
    }

    pub fn from_parts(original: &str, suggestion: &str) -> Self {
        Self {
            original: original.trim().to_string(),
            suggestion: suggestion.trim().to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// CaptureStore trait (port)
// ---------------------------------------------------------------------------

pub trait CaptureStore {
    fn record(&self, record: SuggestionRecord);
    fn mark_accepted(&self, session_id: &str, command: &str, exit_code: i64);
}

// ---------------------------------------------------------------------------
// InMemoryCaptureStore (test double)
// ---------------------------------------------------------------------------

#[cfg(any(test, feature = "testing"))]
pub struct InMemoryCaptureStore {
    inner: std::cell::RefCell<Vec<SuggestionRecord>>,
}

#[cfg(any(test, feature = "testing"))]
impl InMemoryCaptureStore {
    pub fn new() -> Self {
        Self {
            inner: std::cell::RefCell::new(Vec::new()),
        }
    }

    pub fn records(&self) -> Vec<SuggestionRecord> {
        self.inner.borrow().clone()
    }

    pub fn get_records(&self) -> Vec<SuggestionRecord> {
        self.records()
    }
}

#[cfg(any(test, feature = "testing"))]
impl Default for InMemoryCaptureStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "testing"))]
impl CaptureStore for InMemoryCaptureStore {
    fn record(&self, record: SuggestionRecord) {
        self.inner.borrow_mut().push(record);
    }

    fn mark_accepted(&self, session_id: &str, command: &str, exit_code: i64) {
        let mut records = self.inner.borrow_mut();
        for r in records.iter_mut() {
            if r.accepted {
                continue;
            }
            if r.session_id.as_deref() != Some(session_id) {
                continue;
            }
            if r.suggestion.trim() != command.trim() {
                continue;
            }
            r.accepted = true;
            r.accepted_ts = Some(now_iso8601());
            r.exit_code = Some(exit_code);
        }
    }
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

pub struct SuggestionStore {
    pub path: PathBuf,
}

impl SuggestionStore {
    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
            });
        base.join("coursers").join("suggestions.jsonl")
    }

    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Load all records from the JSONL file. Silently skips malformed lines.
    pub fn load(&self) -> Vec<SuggestionRecord> {
        let Ok(file) = std::fs::File::open(&self.path) else {
            return Vec::new();
        };
        std::io::BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(&l).ok())
            .collect()
    }

    /// Record a block event. If the (original, suggestion) pair is new, append.
    /// If it already exists, increment count (and upgrade accepted if applicable).
    pub fn record(&self, record: SuggestionRecord) {
        self.do_record(record);
    }

    /// Mark a pending (unaccepted) record as accepted, matched by session_id +
    /// suggestion text. Updates in-place.
    pub fn mark_accepted(&self, session_id: &str, command: &str, exit_code: i64) {
        self.do_mark_accepted(session_id, command, exit_code);
    }

    fn lock_file(&self) -> Option<std::fs::File> {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&self.path)
            .ok()
    }

    fn do_record(&self, record: SuggestionRecord) {
        let Some(file) = self.lock_file() else { return };
        let mut lock = FdRwLock::new(file);
        let Ok(_guard) = lock.write() else { return };

        let mut records = self.load();
        let key = DedupeKey::from_record(&record);

        if let Some(existing) = records
            .iter_mut()
            .find(|r| DedupeKey::from_record(r) == key)
        {
            existing.count += 1;
            // Upgrade accepted signal if new record carries it.
            if record.accepted && !existing.accepted {
                existing.accepted = true;
                existing.accepted_ts = record.accepted_ts;
                existing.exit_code = record.exit_code;
            }
            self.write_all(&records);
        } else {
            self.append(&record);
        }
    }

    fn do_mark_accepted(&self, session_id: &str, command: &str, exit_code: i64) {
        let Some(file) = self.lock_file() else { return };
        let mut lock = FdRwLock::new(file);
        let Ok(_guard) = lock.write() else { return };

        let mut records = self.load();
        let mut changed = false;

        for r in records.iter_mut() {
            if r.accepted {
                continue;
            }
            if r.session_id.as_deref() != Some(session_id) {
                continue;
            }
            // Accept if the run command matches the suggestion (trimmed).
            if r.suggestion.trim() != command.trim() {
                continue;
            }
            r.accepted = true;
            r.accepted_ts = Some(now_iso8601());
            r.exit_code = Some(exit_code);
            changed = true;
        }

        if changed {
            self.write_all(&records);
        }
    }

    fn append(&self, record: &SuggestionRecord) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        else {
            return;
        };
        if let Ok(line) = serde_json::to_string(record) {
            let _ = writeln!(file, "{}", line);
        }
    }

    fn write_all(&self, records: &[SuggestionRecord]) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.path)
        else {
            return;
        };
        for r in records {
            if let Ok(line) = serde_json::to_string(r) {
                let _ = writeln!(file, "{}", line);
            }
        }
    }
}

impl CaptureStore for SuggestionStore {
    fn record(&self, record: SuggestionRecord) {
        self.do_record(record);
    }

    fn mark_accepted(&self, session_id: &str, command: &str, exit_code: i64) {
        self.do_mark_accepted(session_id, command, exit_code);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Format as ISO 8601 UTC without external deps.
    let (y, mo, d, h, mi, s) = epoch_to_ymd_hms(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

fn epoch_to_ymd_hms(secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let s = secs % 60;
    let total_min = secs / 60;
    let mi = total_min % 60;
    let total_hours = total_min / 60;
    let h = total_hours % 24;
    let mut days = total_hours / 24;

    // Epoch is 1970-01-01
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let month_days: &[u64] = if is_leap(year) {
        &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u64;
    for &md in month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    (year, month, days + 1, h, mi, s)
}

fn is_leap(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

fn repo_from_cwd(cwd: &str) -> Option<String> {
    Path::new(cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn in_memory_store_record_increments_count() {
        let store = InMemoryCaptureStore::new();
        let r = SuggestionRecord::new(
            "grep foo .",
            "rg foo .",
            "no-grep-use-tool",
            "/Users/joe/dev/coursers",
            Some("sess-1".to_string()),
            "Bash",
        );
        store.record(r.clone());
        store.record(r);
        assert_eq!(store.records().len(), 2);
    }

    fn store(dir: &TempDir) -> SuggestionStore {
        SuggestionStore::new(dir.path().join("suggestions.jsonl"))
    }

    fn rec(original: &str, suggestion: &str) -> SuggestionRecord {
        SuggestionRecord::new(
            original,
            suggestion,
            "no-grep-use-tool",
            "/Users/joe/dev/coursers",
            Some("sess-1".to_string()),
            "Bash",
        )
    }

    #[test]
    fn record_new_appends() {
        let dir = TempDir::new().unwrap();
        let s = store(&dir);
        s.record(rec("grep foo .", "rg foo ."));
        let records = s.load();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].original, "grep foo .");
        assert_eq!(records[0].count, 1);
    }

    #[test]
    fn record_duplicate_increments_count() {
        let dir = TempDir::new().unwrap();
        let s = store(&dir);
        s.record(rec("grep foo .", "rg foo ."));
        s.record(rec("grep foo .", "rg foo ."));
        let records = s.load();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].count, 2);
    }

    #[test]
    fn record_different_suggestions_are_separate() {
        let dir = TempDir::new().unwrap();
        let s = store(&dir);
        s.record(rec("grep foo .", "rg foo ."));
        s.record(rec("grep foo .", "Grep(pattern='foo')"));
        let records = s.load();
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn mark_accepted_updates_record() {
        let dir = TempDir::new().unwrap();
        let s = store(&dir);
        s.record(rec("grep foo .", "rg foo ."));
        s.mark_accepted("sess-1", "rg foo .", 0);
        let records = s.load();
        assert!(records[0].accepted);
        assert_eq!(records[0].exit_code, Some(0));
        assert!(records[0].accepted_ts.is_some());
    }

    #[test]
    fn mark_accepted_wrong_session_no_change() {
        let dir = TempDir::new().unwrap();
        let s = store(&dir);
        s.record(rec("grep foo .", "rg foo ."));
        s.mark_accepted("other-session", "rg foo .", 0);
        let records = s.load();
        assert!(!records[0].accepted);
    }

    #[test]
    fn mark_accepted_wrong_command_no_change() {
        let dir = TempDir::new().unwrap();
        let s = store(&dir);
        s.record(rec("grep foo .", "rg foo ."));
        s.mark_accepted("sess-1", "fd foo .", 0);
        let records = s.load();
        assert!(!records[0].accepted);
    }

    #[test]
    fn repo_derived_from_cwd() {
        let r = rec("grep foo .", "rg foo .");
        assert_eq!(r.repo.as_deref(), Some("coursers"));
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let s = SuggestionStore::new(PathBuf::from("/nonexistent/suggestions.jsonl"));
        assert!(s.load().is_empty());
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;
    use tempfile::TempDir;

    fn make_store(dir: &TempDir) -> SuggestionStore {
        SuggestionStore::new(dir.path().join("suggestions.jsonl"))
    }

    proptest! {
        #[test]
        fn dedupe_key_trim_invariant(
            s in "\\s*[^\n]*\\s*",
            t in "\\s*[^\n]*\\s*",
        ) {
            let k1 = DedupeKey::from_parts(s.trim(), t.trim());
            let k2 = DedupeKey::from_parts(&s, &t);
            prop_assert_eq!(k1, k2);
        }

        #[test]
        fn record_n_times_count_equals_n(n in 1usize..=20) {
            let dir = TempDir::new().unwrap();
            let store = make_store(&dir);
            for _ in 0..n {
                store.record(SuggestionRecord::new(
                    "grep foo .", "rg foo .", "no-grep", "/tmp", None, "Bash",
                ));
            }
            let records = store.load();
            prop_assert_eq!(records.len(), 1);
            prop_assert_eq!(records[0].count, n as u32);
        }

        #[test]
        fn mark_accepted_never_decrements_count(n in 1usize..=10) {
            let dir = TempDir::new().unwrap();
            let store = make_store(&dir);
            for _ in 0..n {
                store.record(SuggestionRecord::new(
                    "grep foo .", "rg foo .", "no-grep", "/tmp",
                    Some("sess-1".to_string()), "Bash",
                ));
            }
            let before = store.load()[0].count;
            store.mark_accepted("sess-1", "rg foo .", 0);
            let after = store.load()[0].count;
            prop_assert_eq!(before, after);
        }

        #[test]
        fn accepted_never_downgraded(calls in 1usize..=5) {
            let dir = TempDir::new().unwrap();
            let store = make_store(&dir);
            store.record(SuggestionRecord::new(
                "grep foo .", "rg foo .", "no-grep", "/tmp",
                Some("sess-1".to_string()), "Bash",
            ));
            store.mark_accepted("sess-1", "rg foo .", 0);
            for _ in 0..calls {
                store.mark_accepted("other-sess", "rg foo .", 0);
            }
            prop_assert!(store.load()[0].accepted);
        }
    }

    #[test]
    fn suggestion_record_json_round_trip() {
        use proptest::strategy::Strategy;
        use proptest::test_runner::TestRunner;
        let mut runner = TestRunner::default();
        let strategy = (any::<String>(), any::<String>()).prop_map(|(orig, sug)| {
            SuggestionRecord::new(&orig, &sug, "rule", "/cwd", None, "Bash")
        });
        runner
            .run(&strategy, |record| {
                let json = serde_json::to_string(&record).unwrap();
                let back: SuggestionRecord = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(record.original, back.original);
                prop_assert_eq!(record.suggestion, back.suggestion);
                prop_assert_eq!(record.rule_id, back.rule_id);
                prop_assert_eq!(record.accepted, back.accepted);
                prop_assert_eq!(record.count, back.count);
                Ok(())
            })
            .unwrap();
    }
}
