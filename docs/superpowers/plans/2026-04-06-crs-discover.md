# crs discover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `crs discover` — mines Claude Code JSONL session history and produces a
savings report showing which Bash commands are intercepted by existing rules and which are not.

**Architecture:** Hexagonal. `crs-core` gains a pure `history` module (trait + domain logic,
zero fs deps). The `crs` binary gains a `JsonlCommandSource` adapter that walks `*.jsonl` files
and feeds records into the domain. `main.rs` wires them together and formats the report.

**Tech Stack:** Rust 2024, `walkdir` (new dep on `crs` binary only), `serde_json`, `regex`
(already in workspace), `clap` (already wired).

---

## File Map

| File | Action | Purpose |
|---|---|---|
| `crates/core/src/history.rs` | Create | `CommandSource` trait, `CommandRecord`, `DiscoverOpts`, `DiscoverReport`, `CommandFreq`, `stem_of()`, `discover()` |
| `crates/core/src/lib.rs` | Modify | Add `pub mod history;` |
| `crates/crs/src/jsonl_source.rs` | Create | `JsonlCommandSource` adapter — walks *.jsonl, parses assistant records |
| `crates/crs/src/main.rs` | Modify | Wire `Command::Discover` arm, add `--limit`/`--since`/`--format` args, format report |
| `Cargo.toml` (workspace) | Modify | Add `walkdir = "2"` to `[workspace.dependencies]` |
| `crates/crs/Cargo.toml` | Modify | Add `walkdir = { workspace = true }` to `[dependencies]` |

---

## Task 1: Add `walkdir` to workspace

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/crs/Cargo.toml`

- [ ] **Step 1: Add walkdir to workspace deps**

In `Cargo.toml`, add to `[workspace.dependencies]`:
```toml
walkdir = "2"
```

- [ ] **Step 2: Add walkdir to crs crate deps**

In `crates/crs/Cargo.toml`, add to `[dependencies]`:
```toml
walkdir = { workspace = true }
```

- [ ] **Step 3: Verify it resolves**

```bash
cargo check -p crs 2>&1
```
Expected: `Finished` with no errors (walkdir downloaded and linked).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/crs/Cargo.toml Cargo.lock
git commit -m "chore: add walkdir workspace dependency for crs discover"
```

---

## Task 2: Domain types and `stem_of()` in `crs-core`

**Files:**
- Create: `crates/core/src/history.rs`
- Modify: `crates/core/src/lib.rs`

- [ ] **Step 1: Write failing tests for `stem_of()`**

Create `crates/core/src/history.rs` with just the tests:

```rust
/// Extracts the command stem (1–2 token prefix) used for frequency grouping.
///
/// Rules:
/// 1. Strip leading `KEY=val` env assignments.
/// 2. Strip path prefix from token 0 (keep only the basename).
/// 3. If token 1 exists and does not start with `-`, append it: `cargo nextest`.
///    Otherwise stem = token 0 only.
pub fn stem_of(command: &str) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stem_bare_command() {
        assert_eq!(stem_of("ls -la"), "ls");
    }

    #[test]
    fn stem_two_token_subcommand() {
        assert_eq!(stem_of("cargo nextest run -p crs-core"), "cargo nextest");
    }

    #[test]
    fn stem_subcommand_with_flag_token1() {
        // token 1 starts with `-` → stop at token 0
        assert_eq!(stem_of("git --no-pager log"), "git");
    }

    #[test]
    fn stem_strips_path_prefix() {
        assert_eq!(stem_of("/usr/bin/python3 script.py"), "python3");
    }

    #[test]
    fn stem_strips_env_assignment() {
        assert_eq!(stem_of("RUST_LOG=debug cargo build"), "cargo");
    }

    #[test]
    fn stem_strips_multiple_env_assignments() {
        assert_eq!(stem_of("A=1 B=2 cargo test"), "cargo");
    }

    #[test]
    fn stem_empty_command() {
        assert_eq!(stem_of(""), "");
    }

    #[test]
    fn stem_single_token() {
        assert_eq!(stem_of("make"), "make");
    }

    #[test]
    fn stem_doob_todo() {
        assert_eq!(stem_of("doob todo list --project coursers"), "doob todo");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p crs-core stem_of 2>&1
```
Expected: `FAILED` — `todo!()` panics.

- [ ] **Step 3: Implement `stem_of()`**

Replace the `todo!()` body:

```rust
pub fn stem_of(command: &str) -> String {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    if tokens.is_empty() {
        return String::new();
    }

    // Strip leading KEY=val env assignments
    let start = tokens.iter().take_while(|t| t.contains('=') && !t.starts_with('-')).count();
    let tokens = &tokens[start..];
    if tokens.is_empty() {
        return String::new();
    }

    // Strip path prefix from token 0
    let base = tokens[0].rsplit('/').next().unwrap_or(tokens[0]);

    // Append token 1 if it exists and is not a flag
    if let Some(t1) = tokens.get(1) {
        if !t1.starts_with('-') {
            return format!("{base} {t1}");
        }
    }

    base.to_string()
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p crs-core stem_of 2>&1
```
Expected: `test result: ok. 9 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/history.rs
git commit -m "feat(core): add stem_of() with tests"
```

---

## Task 3: `CommandSource` trait and domain types

**Files:**
- Modify: `crates/core/src/history.rs`
- Modify: `crates/core/src/lib.rs`

- [ ] **Step 1: Add domain types and trait to `history.rs`**

Append to `crates/core/src/history.rs` (before the `#[cfg(test)]` block):

```rust
use std::path::PathBuf;
use crate::rules::Rule;

pub struct CommandRecord {
    pub command: String,
    pub session_id: String,
    pub cwd: String,
    pub timestamp: Option<String>,
}

pub trait CommandSource {
    fn commands(&self) -> impl Iterator<Item = CommandRecord>;
}

pub struct DiscoverOpts {
    pub limit: usize,
    pub since_days: Option<u32>,
    pub all_projects: bool,
    pub current_dir: Option<PathBuf>,
}

impl Default for DiscoverOpts {
    fn default() -> Self {
        Self {
            limit: 15,
            since_days: Some(30),
            all_projects: false,
            current_dir: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct CommandFreq {
    pub stem: String,
    pub count: u64,
    pub example: String,
    pub est_tokens: u64,
    pub rule_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct DiscoverReport {
    pub intercepted: Vec<CommandFreq>,
    pub unhandled: Vec<CommandFreq>,
    pub scanned_sessions: usize,
    pub scanned_commands: usize,
}
```

- [ ] **Step 2: Export `history` from `crs-core`**

In `crates/core/src/lib.rs`, add:
```rust
pub mod history;
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo check -p crs-core 2>&1
```
Expected: `Finished` with no errors.

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/history.rs crates/core/src/lib.rs
git commit -m "feat(core): add CommandSource trait and DiscoverReport types"
```

---

## Task 4: `discover()` domain function

**Files:**
- Modify: `crates/core/src/history.rs`

- [ ] **Step 1: Write failing tests for `discover()`**

Add to the `#[cfg(test)]` block in `history.rs`:

```rust
    // Minimal CommandSource impl for tests
    struct VecSource(Vec<CommandRecord>);
    impl CommandSource for VecSource {
        fn commands(&self) -> impl Iterator<Item = CommandRecord> {
            // CommandRecord is not Clone, so we reconstruct
            self.0.iter().map(|r| CommandRecord {
                command: r.command.clone(),
                session_id: r.session_id.clone(),
                cwd: r.cwd.clone(),
                timestamp: r.timestamp.clone(),
            })
        }
    }

    fn make_record(command: &str, cwd: &str) -> CommandRecord {
        CommandRecord {
            command: command.to_string(),
            session_id: "sess-1".to_string(),
            cwd: cwd.to_string(),
            timestamp: None,
        }
    }

    fn make_rule(id: &str, pattern: &str) -> Rule {
        crate::rules::Rule {
            id: id.to_string(),
            enabled: true,
            pattern: pattern.to_string(),
            pattern_flags: String::new(),
            exceptions: vec![],
            message: None,
        }
    }

    #[test]
    fn discover_counts_unhandled_commands() {
        let src = VecSource(vec![
            make_record("doob todo list", "/project"),
            make_record("doob todo list", "/project"),
            make_record("doob todo list", "/project"),
        ]);
        let report = discover(&src, &[], &DiscoverOpts {
            all_projects: true,
            ..Default::default()
        });
        assert_eq!(report.scanned_commands, 3);
        assert_eq!(report.unhandled.len(), 1);
        assert_eq!(report.unhandled[0].stem, "doob todo");
        assert_eq!(report.unhandled[0].count, 3);
    }

    #[test]
    fn discover_counts_intercepted_commands() {
        let src = VecSource(vec![
            make_record("cargo nextest run", "/project"),
            make_record("cargo nextest run -p foo", "/project"),
        ]);
        let rules = vec![make_rule("no-nextest", r"cargo nextest")];
        let report = discover(&src, &rules, &DiscoverOpts {
            all_projects: true,
            ..Default::default()
        });
        assert_eq!(report.intercepted.len(), 1);
        assert_eq!(report.intercepted[0].stem, "cargo nextest");
        assert_eq!(report.intercepted[0].count, 2);
        assert_eq!(report.intercepted[0].est_tokens, 300); // 2 * 150
    }

    #[test]
    fn discover_filters_by_cwd_when_not_all() {
        let src = VecSource(vec![
            make_record("doob todo", "/project/a"),
            make_record("doob todo", "/project/b"),
        ]);
        let report = discover(&src, &[], &DiscoverOpts {
            all_projects: false,
            current_dir: Some(PathBuf::from("/project/a")),
            ..Default::default()
        });
        assert_eq!(report.scanned_commands, 1);
    }

    #[test]
    fn discover_respects_limit() {
        let src = VecSource((0..20).map(|i| make_record(&format!("cmd{i} sub"), "/p")).collect());
        let report = discover(&src, &[], &DiscoverOpts {
            limit: 5,
            all_projects: true,
            ..Default::default()
        });
        assert!(report.unhandled.len() <= 5);
    }

    #[test]
    fn discover_filters_by_since_days() {
        let old = {
            let mut r = make_record("old cmd", "/p");
            r.timestamp = Some("2020-01-01T00:00:00Z".to_string());
            r
        };
        let new = {
            let mut r = make_record("new cmd", "/p");
            r.timestamp = Some("2099-12-31T00:00:00Z".to_string());
            r
        };
        let src = VecSource(vec![old, new]);
        let report = discover(&src, &[], &DiscoverOpts {
            since_days: Some(30),
            all_projects: true,
            ..Default::default()
        });
        // Only the future-dated record passes the filter in test context;
        // implementation compares date strings — "2099" > today's date string
        assert_eq!(report.scanned_commands, 1);
        assert_eq!(report.unhandled[0].stem, "new cmd");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p crs-core discover 2>&1
```
Expected: compile error — `discover` not defined yet.

- [ ] **Step 3: Implement `discover()`**

Add before the `#[cfg(test)]` block in `history.rs`:

```rust
use std::collections::HashMap;
use regex::Regex;

pub fn discover(
    source: &impl CommandSource,
    rules: &[Rule],
    opts: &DiscoverOpts,
) -> DiscoverReport {
    let today = chrono_today(); // YYYY-MM-DD string
    let cutoff: Option<String> = opts.since_days.map(|d| {
        // Compute cutoff date string by subtracting days from today
        days_ago(&today, d)
    });

    let mut intercepted: HashMap<String, CommandFreq> = HashMap::new();
    let mut unhandled: HashMap<String, CommandFreq> = HashMap::new();
    let mut scanned_commands = 0usize;
    let mut seen_sessions = std::collections::HashSet::new();

    for rec in source.commands() {
        // Project filter
        if !opts.all_projects {
            if let Some(ref cwd) = opts.current_dir {
                if rec.cwd != cwd.to_string_lossy().as_ref() {
                    continue;
                }
            }
        }

        // Since filter — compare date prefix (first 10 chars of ISO 8601)
        if let (Some(ref cutoff_str), Some(ref ts)) = (&cutoff, &rec.timestamp) {
            let date_part = &ts[..ts.len().min(10)];
            if date_part < cutoff_str.as_str() {
                continue;
            }
        }

        scanned_commands += 1;
        seen_sessions.insert(rec.session_id.clone());

        let stem = stem_of(&rec.command);
        if stem.is_empty() {
            continue;
        }

        // Check against rules
        let matched_rule = rules.iter().find(|r| {
            if !r.enabled { return false; }
            let pat = if r.pattern_flags.contains('i') {
                format!("(?i){}", r.pattern)
            } else {
                r.pattern.clone()
            };
            Regex::new(&pat).map(|re| re.is_match(&rec.command)).unwrap_or(false)
        });

        let bucket = if matched_rule.is_some() { &mut intercepted } else { &mut unhandled };
        let entry = bucket.entry(stem.clone()).or_insert_with(|| CommandFreq {
            stem: stem.clone(),
            count: 0,
            example: rec.command.clone(),
            est_tokens: 0,
            rule_id: matched_rule.map(|r| r.id.clone()),
        });
        entry.count += 1;
        entry.est_tokens = entry.count * 150;
    }

    // Sort by count desc, truncate to limit
    let mut intercepted: Vec<CommandFreq> = intercepted.into_values().collect();
    let mut unhandled: Vec<CommandFreq> = unhandled.into_values().collect();
    intercepted.sort_by(|a, b| b.count.cmp(&a.count));
    unhandled.sort_by(|a, b| b.count.cmp(&a.count));
    intercepted.truncate(opts.limit);
    unhandled.truncate(opts.limit);

    DiscoverReport {
        intercepted,
        unhandled,
        scanned_sessions: seen_sessions.len(),
        scanned_commands,
    }
}

/// Returns today's date as YYYY-MM-DD using only std (no chrono dep).
fn chrono_today() -> String {
    // Use the system time and manual calculation to avoid adding chrono to crs-core.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    unix_secs_to_date(secs)
}

fn days_ago(today: &str, days: u32) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cutoff_secs = secs.saturating_sub(days as u64 * 86400);
    unix_secs_to_date(cutoff_secs)
}

fn unix_secs_to_date(secs: u64) -> String {
    // Gregorian calendar calculation (no leap second handling needed for date comparison)
    let days_since_epoch = secs / 86400;
    let mut remaining = days_since_epoch + 719468; // offset to year 0
    let era = remaining / 146097;
    remaining %= 146097;
    let yoe = (remaining - remaining / 1460 + remaining / 36524 - remaining / 146096) / 365;
    let y = yoe + era * 400;
    let doy = remaining - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p crs-core discover 2>&1
```
Expected: `test result: ok. 5 passed`.

- [ ] **Step 5: Run full test suite**

```bash
cargo test -p crs-core 2>&1
```
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/history.rs
git commit -m "feat(core): implement discover() domain function"
```

---

## Task 5: `JsonlCommandSource` adapter

**Files:**
- Create: `crates/crs/src/jsonl_source.rs`
- Modify: `crates/crs/src/main.rs` (add `mod jsonl_source;`)

- [ ] **Step 1: Write failing integration test**

Create `crates/crs/tests/discover_integration.rs`:

```rust
use crs_core::history::{CommandSource, DiscoverOpts, discover};
use crs_core::rules::load as load_rules;

// The fixture dir already exists at crates/crs/tests/fixtures/
// We'll add a discover fixture in Task 6. This test just verifies
// JsonlCommandSource compiles and yields zero records for an empty dir.
#[test]
fn jsonl_source_empty_dir_yields_no_commands() {
    let tmp = tempfile::tempdir().unwrap();
    let src = crs::jsonl_source::JsonlCommandSource::new(
        tmp.path().to_path_buf(),
        false,
        std::env::current_dir().ok(),
    );
    let rules = load_rules();
    let report = discover(&src, &rules.rules, &DiscoverOpts {
        all_projects: true,
        ..Default::default()
    });
    assert_eq!(report.scanned_commands, 0);
}
```

Add `tempfile` to `crates/crs/Cargo.toml` `[dev-dependencies]`:
```toml
[dev-dependencies]
tempfile = { workspace = true }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p crs discover_integration 2>&1
```
Expected: compile error — `crs::jsonl_source` not found.

- [ ] **Step 3: Implement `JsonlCommandSource`**

Create `crates/crs/src/jsonl_source.rs`:

```rust
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
                    .filter_map(|line| {
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

                        // Project filter applied here for efficiency
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
```

Add `mod jsonl_source;` to `crates/crs/src/main.rs` and expose it:
```rust
pub mod jsonl_source;
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test -p crs jsonl_source_empty_dir_yields_no_commands 2>&1
```
Expected: `test result: ok. 1 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/crs/src/jsonl_source.rs crates/crs/src/main.rs crates/crs/Cargo.toml
git commit -m "feat(crs): add JsonlCommandSource adapter"
```

---

## Task 6: Fixture-based integration test

**Files:**
- Create: `crates/crs/tests/fixtures/discover/session.jsonl`
- Modify: `crates/crs/tests/discover_integration.rs`

- [ ] **Step 1: Create discover fixture JSONL**

Create `crates/crs/tests/fixtures/discover/session.jsonl` with two assistant records
containing Bash tool_use blocks:

```jsonl
{"type":"assistant","sessionId":"test-sess","cwd":"/test/project","timestamp":"2099-01-01T00:00:00Z","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","input":{"command":"cargo nextest run -p crs-core"}}]}}
{"type":"assistant","sessionId":"test-sess","cwd":"/test/project","timestamp":"2099-01-01T00:00:00Z","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","input":{"command":"doob todo list --project coursers"}}]}}
{"type":"assistant","sessionId":"test-sess","cwd":"/test/project","timestamp":"2099-01-01T00:00:00Z","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","input":{"command":"doob todo list --project minibox"}}]}}
{"type":"user","sessionId":"test-sess","cwd":"/test/project","message":{"role":"user","content":[]}}
```

- [ ] **Step 2: Write fixture test**

Add to `crates/crs/tests/discover_integration.rs`:

```rust
use std::path::PathBuf;

#[test]
fn jsonl_source_reads_fixture_commands() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/discover");

    let src = crs::jsonl_source::JsonlCommandSource::new(
        fixtures,
        true,
        None,
    );

    let rules = vec![]; // no rules — all unhandled
    let report = crs_core::history::discover(
        &src,
        &rules,
        &crs_core::history::DiscoverOpts {
            all_projects: true,
            since_days: None, // no date filter — fixture uses 2099
            ..Default::default()
        },
    );

    assert_eq!(report.scanned_commands, 3);
    assert_eq!(report.scanned_sessions, 1);
    // doob todo appears twice → top of unhandled
    assert_eq!(report.unhandled[0].stem, "doob todo");
    assert_eq!(report.unhandled[0].count, 2);
}
```

- [ ] **Step 3: Run test**

```bash
cargo test -p crs jsonl_source_reads_fixture_commands 2>&1
```
Expected: `test result: ok. 1 passed`.

- [ ] **Step 4: Commit**

```bash
git add crates/crs/tests/fixtures/discover/session.jsonl crates/crs/tests/discover_integration.rs
git commit -m "test(crs): fixture-based discover integration test"
```

---

## Task 7: Wire `Command::Discover` in `main.rs`

**Files:**
- Modify: `crates/crs/src/main.rs`

- [ ] **Step 1: Expand CLI args for Discover**

Replace the existing `Discover` variant in `main.rs`:

```rust
/// Discover missed savings from Claude Code session history
Discover {
    /// Scan all projects (default: current project only)
    #[arg(short, long)]
    all: bool,
    /// Max rows per section
    #[arg(short, long, default_value = "15")]
    limit: usize,
    /// Scan sessions from last N days
    #[arg(short, long, default_value = "30")]
    since: u32,
    /// Output format: text or json
    #[arg(short, long, default_value = "text")]
    format: String,
},
```

- [ ] **Step 2: Implement `cmd_discover()`**

Add this function to `main.rs`:

```rust
fn cmd_discover(all: bool, limit: usize, since: u32, format: &str) {
    use crs_core::history::{DiscoverOpts, discover};
    use crs_core::rules::load as load_rules;

    let root = std::env::var("CLAUDE_PROJECTS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .expect("home dir")
                .join(".claude/projects")
        });

    let current_dir = std::env::current_dir().ok();
    let src = jsonl_source::JsonlCommandSource::new(root, all, current_dir.clone());

    let rules_cfg = load_rules();
    let opts = DiscoverOpts {
        limit,
        since_days: Some(since),
        all_projects: all,
        current_dir,
    };

    let report = discover(&src, &rules_cfg.rules, &opts);

    match format {
        "json" => print_discover_json(&report),
        _ => print_discover_text(&report),
    }
}

fn print_discover_text(report: &crs_core::history::DiscoverReport) {
    println!("CRS Discover — Savings Opportunities");
    println!("{}", "=".repeat(52));
    println!(
        "Scanned: {} sessions, {} Bash commands\n",
        report.scanned_sessions, report.scanned_commands
    );

    if !report.intercepted.is_empty() {
        println!("INTERCEPTED — commands with matching rules");
        println!("{}", "-".repeat(72));
        println!("{:<24} {:>6}   {:<24} {:>12}", "Command", "Count", "Rule", "Est. Savings");
        for f in &report.intercepted {
            let rule = f.rule_id.as_deref().unwrap_or("-");
            let savings = format_tokens(f.est_tokens);
            println!("{:<24} {:>6}   {:<24} {:>12}", f.stem, f.count, rule, savings);
        }
        let total_tokens: u64 = report.intercepted.iter().map(|f| f.est_tokens).sum();
        let total_cmds: u64 = report.intercepted.iter().map(|f| f.count).sum();
        println!("{}", "-".repeat(72));
        println!("Total: {} commands → {}", total_cmds, format_tokens(total_tokens));
    }

    if !report.unhandled.is_empty() {
        println!("\nTOP UNHANDLED — no matching rule");
        println!("{}", "-".repeat(52));
        println!("{:<24} {:>6}   {}", "Command", "Count", "Example");
        for f in &report.unhandled {
            let ex = if f.example.len() > 36 {
                format!("{}...", &f.example[..36])
            } else {
                f.example.clone()
            };
            println!("{:<24} {:>6}   {}", f.stem, f.count, ex);
        }
        println!("{}", "-".repeat(52));
    }

    println!("~estimated at 150 tokens/intercept");
}

fn print_discover_json(report: &crs_core::history::DiscoverReport) {
    let out = serde_json::json!({
        "scanned_sessions": report.scanned_sessions,
        "scanned_commands": report.scanned_commands,
        "intercepted": report.intercepted.iter().map(|f| serde_json::json!({
            "stem": f.stem,
            "count": f.count,
            "example": f.example,
            "est_tokens": f.est_tokens,
            "rule_id": f.rule_id,
        })).collect::<Vec<_>>(),
        "unhandled": report.unhandled.iter().map(|f| serde_json::json!({
            "stem": f.stem,
            "count": f.count,
            "example": f.example,
        })).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

fn format_tokens(n: u64) -> String {
    if n >= 1000 {
        format!("~{:.1}K tokens", n as f64 / 1000.0)
    } else {
        format!("~{n} tokens")
    }
}
```

- [ ] **Step 3: Wire into `main()`**

Replace the `Command::Discover { all }` arm:

```rust
Command::Discover { all, limit, since, format } => {
    cmd_discover(all, limit, since, &format);
}
```

- [ ] **Step 4: Build and smoke test**

```bash
cargo build -p crs 2>&1
```
Expected: `Finished` with no errors.

```bash
./target/debug/crs discover --all --since 1 2>&1 | head -5
```
Expected: first line is `CRS Discover — Savings Opportunities`.

- [ ] **Step 5: Run full test suite**

```bash
cargo test 2>&1
```
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/crs/src/main.rs
git commit -m "feat(crs): wire crs discover subcommand with text and json output"
```

---

## Task 8: Update HANDOFF.yaml

- [ ] **Step 1: Mark coursers-4 done and run full suite one final time**

```bash
cargo test 2>&1
```
Expected: all tests pass.

- [ ] **Step 2: Commit final state**

```bash
git add -A
git commit -m "feat: implement crs discover — savings report from JSONL history"
```
