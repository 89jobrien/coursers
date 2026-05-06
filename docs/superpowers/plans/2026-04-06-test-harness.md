---
status: done
---

# Test Harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a full test harness for the `coursers` workspace — trait-extracted core,
unit tests on all domain logic, integration tests that spawn real binaries, and a Nushell
smoke script.

**Architecture:** Extract `RulesLoader` and `StateStore` traits into `crs-core` so hook logic
can be tested with in-memory fakes. Workspace-level integration tests spawn the `coursers`
binary via `std::process::Command` with fixture payloads. Smoke script validates end-to-end
against the live binary.

**Tech Stack:** Rust 2024 edition, `cargo test`, `tempfile` crate for temp dirs in integration
tests, Nushell for smoke script.

---

## Task 1: Cleanup — delete dead code and add tempfile dependency

**Files:**

- Delete: `hooks/pre-tool-course-correct.nu`
- Delete: `hooks/post-tool-track-failures.nu`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Stage the already-deleted `src/` files and delete `hooks/`**

```bash
git rm src/config.rs src/hook/mod.rs src/hook/post.rs src/hook/pre.rs src/main.rs src/rules.rs src/state.rs
git rm hooks/pre-tool-course-correct.nu hooks/post-tool-track-failures.nu
```

Expected: staged deletions, no errors.

- [ ] **Step 2: Add `tempfile` to workspace dev-dependencies**

In `Cargo.toml` (workspace root), add after `[workspace.dependencies]`:

```toml
[workspace.dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
regex = "1"
sha2 = "0.10"
hex = "0.4"
dirs = "5"
tempfile = "3"
```

- [ ] **Step 3: Verify workspace builds**

```bash
cargo check --workspace
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml
git commit -m "chore: remove dead hooks/ and src/, add tempfile workspace dep"
```

---

## Task 2: Add `RulesLoader` trait and `FsRulesLoader` to `crs-core`

**Files:**

- Create: `crates/core/src/loader.rs`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/core/Cargo.toml`

- [ ] **Step 1: Add `testing` feature to `crates/core/Cargo.toml`**

```toml
[package]
name = "crs-core"
description = "Shared config, rules, and state for coursers and crs"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[features]
testing = []

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
regex = { workspace = true }
dirs = { workspace = true }
sha2 = { workspace = true }
hex = { workspace = true }
```

- [ ] **Step 2: Create `crates/core/src/loader.rs`**

```rust
use crate::rules::{load as fs_load, RulesConfig};

pub trait RulesLoader {
    fn load(&self) -> RulesConfig;
}

/// Loads rules from the filesystem (COURSERS_RULES env var or default path).
pub struct FsRulesLoader;

impl RulesLoader for FsRulesLoader {
    fn load(&self) -> RulesConfig {
        fs_load()
    }
}

/// In-memory loader for tests. Returns the config it was constructed with.
#[cfg(any(test, feature = "testing"))]
pub struct InMemoryRulesLoader(pub RulesConfig);

#[cfg(any(test, feature = "testing"))]
impl RulesLoader for InMemoryRulesLoader {
    fn load(&self) -> RulesConfig {
        RulesConfig {
            rules: self.0.rules.clone(),
            failure_learning: crate::rules::FailureLearning {
                enabled: self.0.failure_learning.enabled,
                block_threshold: self.0.failure_learning.block_threshold,
                window_seconds: self.0.failure_learning.window_seconds,
                state_file: self.0.failure_learning.state_file.clone(),
                max_tracked_commands: self.0.failure_learning.max_tracked_commands,
                cleanup_after_seconds: self.0.failure_learning.cleanup_after_seconds,
                message_template: self.0.failure_learning.message_template.clone(),
            },
        }
    }
}
```

- [ ] **Step 3: Expose `loader` in `crates/core/src/lib.rs`**

```rust
pub mod config;
pub mod loader;
pub mod rules;
pub mod state;
```

- [ ] **Step 4: Verify it compiles**

```bash
cargo check -p crs-core
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add crates/core/
git commit -m "feat(core): add RulesLoader trait and FsRulesLoader"
```

---

## Task 3: Add `StateStore` trait and `FsStateStore` to `crs-core`

**Files:**

- Create: `crates/core/src/store.rs`
- Modify: `crates/core/src/lib.rs`

- [ ] **Step 1: Create `crates/core/src/store.rs`**

```rust
use std::cell::RefCell;
use std::path::{Path, PathBuf};

use crate::state::{load as fs_load, save as fs_save, State};

pub trait StateStore {
    fn load(&self) -> State;
    fn save(&self, state: &State);
    fn path(&self) -> &Path;
}

/// Reads/writes state JSON to a real file path.
pub struct FsStateStore {
    pub path: PathBuf,
}

impl StateStore for FsStateStore {
    fn load(&self) -> State {
        fs_load(&self.path)
    }

    fn save(&self, state: &State) {
        fs_save(&self.path, state);
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

/// In-memory store for tests. No filesystem I/O.
#[cfg(any(test, feature = "testing"))]
pub struct InMemoryStateStore {
    inner: RefCell<State>,
    path: PathBuf,
}

#[cfg(any(test, feature = "testing"))]
impl InMemoryStateStore {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(State::default()),
            path: PathBuf::from("/tmp/in-memory-state.json"),
        }
    }

    pub fn with_state(state: State) -> Self {
        Self {
            inner: RefCell::new(state),
            path: PathBuf::from("/tmp/in-memory-state.json"),
        }
    }

    pub fn get_state(&self) -> State {
        self.inner.borrow().clone()
    }
}

#[cfg(any(test, feature = "testing"))]
impl StateStore for InMemoryStateStore {
    fn load(&self) -> State {
        self.inner.borrow().clone()
    }

    fn save(&self, state: &State) {
        *self.inner.borrow_mut() = state.clone();
    }

    fn path(&self) -> &Path {
        &self.path
    }
}
```

- [ ] **Step 2: `State` needs `Clone` — add derive to `crates/core/src/state.rs`**

Change:

```rust
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct FailureEntry {
```

To:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FailureEntry {
```

And:

```rust
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct State {
```

To:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
```

- [ ] **Step 3: Expose `store` in `crates/core/src/lib.rs`**

```rust
pub mod config;
pub mod loader;
pub mod rules;
pub mod state;
pub mod store;
```

- [ ] **Step 4: Verify it compiles**

```bash
cargo check -p crs-core
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add crates/core/
git commit -m "feat(core): add StateStore trait, FsStateStore, InMemoryStateStore"
```

---

## Task 4: Refactor `hook::pre` and `hook::post` to use traits

**Files:**

- Modify: `crates/coursers/src/hook/pre.rs`
- Modify: `crates/coursers/src/hook/post.rs`
- Modify: `crates/coursers/src/main.rs`
- Modify: `crates/coursers/Cargo.toml`

- [ ] **Step 1: Add `crs-core` testing feature to `crates/coursers/Cargo.toml`**

```toml
[package]
name = "coursers"
description = "Claude Code PreToolUse/PostToolUse hook — command blocking and failure learning"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[[bin]]
name = "coursers"
path = "src/main.rs"

[dependencies]
crs-core = { path = "../core" }
clap = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
regex = { workspace = true }

[dev-dependencies]
crs-core = { path = "../core", features = ["testing"] }
```

- [ ] **Step 2: Rewrite `crates/coursers/src/hook/pre.rs`**

```rust
use crs_core::loader::RulesLoader;
use crs_core::store::StateStore;
use crs_core::{rules, state};

use super::{deny, HookPayload};

pub fn run_with<L: RulesLoader, S: StateStore>(loader: &L, store: &S, payload: &HookPayload) {
    if payload.tool_name.as_deref() != Some("Bash") {
        return;
    }

    let command = match payload.tool_input.as_ref().and_then(|i| i.command.as_deref()) {
        Some(c) if !c.is_empty() => c,
        _ => return,
    };

    let config = loader.load();

    // 1. Predefined rules
    if let Some(msg) = rules::check(command, &config.rules) {
        deny(&msg);
    }

    // 2. Learned failures
    let fl = &config.failure_learning;
    if fl.enabled {
        let st = store.load();
        if let Some(msg) = state::check_learned(command, &st, fl) {
            deny(&msg);
        }
    }
}

pub fn run() {
    use crs_core::loader::FsRulesLoader;
    use crs_core::store::FsStateStore;
    use crs_core::state::state_path;

    let Some(payload) = super::read_stdin() else {
        return;
    };

    let loader = FsRulesLoader;
    let config = loader.load();
    let path = state_path(&config.failure_learning);
    let store = FsStateStore { path };

    run_with(&loader, &store, &payload);
}
```

- [ ] **Step 3: Rewrite `crates/coursers/src/hook/post.rs`**

```rust
use crs_core::loader::RulesLoader;
use crs_core::store::StateStore;
use crs_core::state;

use super::HookPayload;

const SIGNAL_EXIT_CODES: &[i64] = &[130, 137, 143];
const EXCLUDE_PATTERNS: &[&str] = &[
    r"^\s*false\s*$",
    r"\|\|\s*(true|:)\s*$",
    r";\s*(true|:)\s*$",
    r"^\s*\[",
    r"\btest\s+-[defhlrswxz]\b",
    r"2>/dev/null",
    r">/dev/null\s+2>&1",
];

pub fn run_with<L: RulesLoader, S: StateStore>(loader: &L, store: &S, payload: &HookPayload) {
    if payload.tool_name.as_deref() != Some("Bash") {
        return;
    }

    let exit_code = payload
        .tool_response
        .as_ref()
        .and_then(|r| r.get("exit_code"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    if exit_code == 0 {
        return;
    }
    if SIGNAL_EXIT_CODES.contains(&exit_code) {
        return;
    }

    let command = match payload.tool_input.as_ref().and_then(|i| i.command.as_deref()) {
        Some(c) if !c.is_empty() => c,
        _ => return,
    };

    if is_excluded(command) {
        return;
    }

    let config = loader.load();
    let fl = &config.failure_learning;
    if !fl.enabled {
        return;
    }

    let st = store.load();
    let st = state::record_failure(st, command, fl);
    store.save(&st);
}

pub fn run() {
    use crs_core::loader::FsRulesLoader;
    use crs_core::store::FsStateStore;
    use crs_core::state::state_path;

    let Some(payload) = super::read_stdin() else {
        return;
    };

    let loader = FsRulesLoader;
    let config = loader.load();
    let path = state_path(&config.failure_learning);
    let store = FsStateStore { path };

    run_with(&loader, &store, &payload);
}

fn is_excluded(command: &str) -> bool {
    EXCLUDE_PATTERNS.iter().any(|pat| {
        regex::Regex::new(pat)
            .map(|re| re.is_match(command))
            .unwrap_or(false)
    })
}
```

- [ ] **Step 4: Verify it compiles**

```bash
cargo check --workspace
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add crates/coursers/
git commit -m "refactor(coursers): inject RulesLoader+StateStore into hook run_with fns"
```

---

## Task 5: Unit tests for `crates/core/src/rules.rs`

**Files:**

- Modify: `crates/core/src/rules.rs`

- [ ] **Step 1: Add tests module to `crates/core/src/rules.rs`**

Append to the end of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_rule(id: &str, pattern: &str) -> Rule {
        Rule {
            id: id.to_string(),
            enabled: true,
            pattern: pattern.to_string(),
            pattern_flags: String::new(),
            exceptions: vec![],
            message: None,
        }
    }

    #[test]
    fn rule_matches_pattern() {
        let rules = vec![make_rule("no-grep", r"\bgrep\b")];
        let result = check("grep foo .", &rules);
        assert!(result.is_some());
        assert!(result.unwrap().contains("no-grep"));
    }

    #[test]
    fn rule_no_match() {
        let rules = vec![make_rule("no-grep", r"\bgrep\b")];
        assert!(check("ls -la", &rules).is_none());
    }

    #[test]
    fn rule_case_insensitive_flag() {
        let mut rule = make_rule("no-grep", r"\bgrep\b");
        rule.pattern_flags = "i".to_string();
        assert!(check("GREP foo .", &[rule]).is_some());
    }

    #[test]
    fn rule_exception_bypasses_block() {
        let mut rule = make_rule("no-grep", r"\bgrep\b");
        rule.exceptions = vec![r"\| grep".to_string()];
        assert!(check("cmd | grep foo", &[rule]).is_none());
    }

    #[test]
    fn rule_disabled_skipped() {
        let mut rule = make_rule("no-grep", r"\bgrep\b");
        rule.enabled = false;
        assert!(check("grep foo .", &[rule]).is_none());
    }

    #[test]
    fn rule_bad_regex_skipped() {
        let rule = make_rule("bad", r"[invalid");
        // must not panic
        assert!(check("anything", &[rule]).is_none());
    }

    #[test]
    fn no_rules_allows_all() {
        assert!(check("grep foo .", &[]).is_none());
    }

    #[test]
    fn rule_custom_message_returned() {
        let mut rule = make_rule("no-grep", r"\bgrep\b");
        rule.message = Some("Use the Grep tool.".to_string());
        assert_eq!(check("grep foo .", &[rule]).unwrap(), "Use the Grep tool.");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p crs-core rules
```

Expected: 8 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/rules.rs
git commit -m "test(core): unit tests for rules::check"
```

---

## Task 6: Unit tests for `crates/core/src/state.rs`

**Files:**

- Modify: `crates/core/src/state.rs`

- [ ] **Step 1: Add tests module to `crates/core/src/state.rs`**

Append to the end of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::FailureLearning;

    fn fl(threshold: usize, window: u64) -> FailureLearning {
        FailureLearning {
            enabled: true,
            block_threshold: threshold,
            window_seconds: window,
            state_file: None,
            max_tracked_commands: 200,
            cleanup_after_seconds: 3600,
            message_template: None,
        }
    }

    #[test]
    fn record_failure_creates_entry() {
        let st = record_failure(State::default(), "grep foo .", &fl(3, 300));
        assert_eq!(st.failures.len(), 1);
        let entry = st.failures.values().next().unwrap();
        assert_eq!(entry.timestamps.len(), 1);
    }

    #[test]
    fn record_failure_appends() {
        let st = record_failure(State::default(), "grep foo .", &fl(3, 300));
        let st = record_failure(st, "grep foo .", &fl(3, 300));
        let entry = st.failures.values().next().unwrap();
        assert_eq!(entry.timestamps.len(), 2);
    }

    #[test]
    fn prune_removes_old_timestamps() {
        let mut st = State::default();
        let key = command_key("grep foo .");
        st.failures.insert(key, FailureEntry {
            command_preview: "grep foo .".to_string(),
            timestamps: vec![0], // ancient timestamp
            last_seen: 0.0,
        });
        let fl = FailureLearning {
            enabled: true,
            block_threshold: 3,
            window_seconds: 300,
            state_file: None,
            max_tracked_commands: 200,
            cleanup_after_seconds: 3600,
            message_template: None,
        };
        // recording a new failure triggers prune; old timestamps outside window are dropped
        let st = record_failure(st, "grep foo .", &fl);
        let entry = st.failures.values().next().unwrap();
        // only the new timestamp remains (old one at t=0 is outside 300s window)
        assert_eq!(entry.timestamps.len(), 1);
    }

    #[test]
    fn prune_evicts_over_max() {
        let mut st = State::default();
        let now = now_secs();
        let fl_cfg = FailureLearning {
            enabled: true,
            block_threshold: 3,
            window_seconds: 300,
            state_file: None,
            max_tracked_commands: 5,
            cleanup_after_seconds: 3600,
            message_template: None,
        };
        for i in 0..5u64 {
            let cmd = format!("cmd-{i}");
            let key = command_key(&cmd);
            st.failures.insert(key, FailureEntry {
                command_preview: cmd,
                timestamps: vec![now - i], // older entries have smaller timestamps
                last_seen: (now - i) as f64,
            });
        }
        let st = record_failure(st, "cmd-new", &fl_cfg);
        assert!(st.failures.len() <= 5);
    }

    #[test]
    fn check_learned_below_threshold() {
        let mut st = State::default();
        let now = now_secs();
        let key = command_key("grep foo .");
        st.failures.insert(key, FailureEntry {
            command_preview: "grep foo .".to_string(),
            timestamps: vec![now, now],
            last_seen: now as f64,
        });
        assert!(check_learned("grep foo .", &st, &fl(3, 300)).is_none());
    }

    #[test]
    fn check_learned_at_threshold() {
        let mut st = State::default();
        let now = now_secs();
        let key = command_key("grep foo .");
        st.failures.insert(key, FailureEntry {
            command_preview: "grep foo .".to_string(),
            timestamps: vec![now, now, now],
            last_seen: now as f64,
        });
        assert!(check_learned("grep foo .", &st, &fl(3, 300)).is_some());
    }

    #[test]
    fn check_learned_disabled() {
        let mut st = State::default();
        let now = now_secs();
        let key = command_key("grep foo .");
        st.failures.insert(key, FailureEntry {
            command_preview: "grep foo .".to_string(),
            timestamps: vec![now, now, now],
            last_seen: now as f64,
        });
        let mut fl_cfg = fl(3, 300);
        fl_cfg.enabled = false;
        assert!(check_learned("grep foo .", &st, &fl_cfg).is_none());
    }

    #[test]
    fn cleanup_removes_stale_entries() {
        let mut st = State::default();
        let key = command_key("grep foo .");
        st.failures.insert(key, FailureEntry {
            command_preview: "grep foo .".to_string(),
            timestamps: vec![],
            last_seen: 0.0, // ancient
        });
        // record a different command to trigger prune
        let fl_cfg = FailureLearning {
            enabled: true,
            block_threshold: 3,
            window_seconds: 300,
            state_file: None,
            max_tracked_commands: 200,
            cleanup_after_seconds: 1, // 1 second cleanup
            message_template: None,
        };
        let st = record_failure(st, "other-cmd", &fl_cfg);
        // ancient entry should be removed
        assert!(!st.failures.contains_key(&command_key("grep foo .")));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p crs-core state
```

Expected: 8 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/state.rs
git commit -m "test(core): unit tests for state — record, prune, check_learned"
```

---

## Task 7: Unit tests for `crates/core/src/config.rs`

**Files:**

- Modify: `crates/core/src/config.rs`

- [ ] **Step 1: Add tests to `crates/core/src/config.rs`**

Append to the end of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_var_overrides_default_rules_path() {
        // Safety: test-only env mutation
        unsafe { std::env::set_var("COURSERS_RULES", "/tmp/test-rules.json") };
        let path = rules_path();
        unsafe { std::env::remove_var("COURSERS_RULES") };
        assert_eq!(path.to_str().unwrap(), "/tmp/test-rules.json");
    }

    #[test]
    fn default_rules_path_contains_claude() {
        unsafe { std::env::remove_var("COURSERS_RULES") };
        let path = rules_path();
        assert!(path.to_string_lossy().contains(".claude"));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p crs-core config
```

Expected: 2 tests pass.

Note: these tests mutate env vars. If run in parallel with other env-touching tests they can
interfere. They're in `crs-core` which has no other env tests, so this is safe. In a larger
workspace you would use `serial_test` crate — not needed here.

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/config.rs
git commit -m "test(core): unit tests for config path resolution"
```

---

## Task 8: Unit tests for `hook::pre` in `crates/coursers`

**Files:**

- Modify: `crates/coursers/src/hook/pre.rs`

- [ ] **Step 1: Append tests to `crates/coursers/src/hook/pre.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crs_core::loader::InMemoryRulesLoader;
    use crs_core::rules::{FailureLearning, Rule, RulesConfig};
    use crs_core::state::{command_key, FailureEntry, State};
    use crs_core::store::InMemoryStateStore;
    use super::super::{HookPayload, ToolInput};
    use std::collections::HashMap;

    fn bash_payload(cmd: &str) -> HookPayload {
        HookPayload {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(ToolInput { command: Some(cmd.to_string()) }),
            tool_response: None,
        }
    }

    fn read_payload() -> HookPayload {
        HookPayload {
            tool_name: Some("Read".to_string()),
            tool_input: None,
            tool_response: None,
        }
    }

    fn empty_config() -> RulesConfig {
        RulesConfig {
            rules: vec![],
            failure_learning: FailureLearning::default(),
        }
    }

    fn config_with_rule(pattern: &str) -> RulesConfig {
        RulesConfig {
            rules: vec![Rule {
                id: "test-rule".to_string(),
                enabled: true,
                pattern: pattern.to_string(),
                pattern_flags: String::new(),
                exceptions: vec![],
                message: Some("blocked".to_string()),
            }],
            failure_learning: FailureLearning::default(),
        }
    }

    fn state_with_failures(cmd: &str, count: usize) -> State {
        let now = crs_core::state::now_secs();
        let key = command_key(cmd);
        let mut failures = HashMap::new();
        failures.insert(key, FailureEntry {
            command_preview: cmd.to_string(),
            timestamps: vec![now; count],
            last_seen: now as f64,
        });
        State { failures }
    }

    // run_with returns normally (no deny) when the command is allowed.
    // We capture "no deny" by ensuring the function returns without calling std::process::exit.
    // The deny() fn calls exit(2), so if the test process exits that's a failure.
    // We test the allow path by verifying run_with completes.

    #[test]
    fn non_bash_tool_passthrough() {
        let loader = InMemoryRulesLoader(config_with_rule(r"\bgrep\b"));
        let store = InMemoryStateStore::new();
        // Should return without panicking or exiting
        run_with(&loader, &store, &read_payload());
    }

    #[test]
    fn empty_command_passthrough() {
        let loader = InMemoryRulesLoader(config_with_rule(r"\bgrep\b"));
        let store = InMemoryStateStore::new();
        let payload = HookPayload {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(ToolInput { command: Some(String::new()) }),
            tool_response: None,
        };
        run_with(&loader, &store, &payload);
    }

    #[test]
    fn allowed_command_completes() {
        let loader = InMemoryRulesLoader(config_with_rule(r"\bgrep\b"));
        let store = InMemoryStateStore::new();
        run_with(&loader, &store, &bash_payload("ls -la"));
    }

    #[test]
    fn failure_learning_disabled_allows_at_threshold() {
        let mut config = empty_config();
        config.failure_learning.enabled = false;
        let loader = InMemoryRulesLoader(config);
        let store = InMemoryStateStore::with_state(state_with_failures("grep foo .", 5));
        // should not deny — learning disabled
        run_with(&loader, &store, &bash_payload("grep foo ."));
    }
}
```

Note: tests for the deny path (`rule_match_denies`, `learned_failure_at_threshold_denies`)
cannot be unit tested here because `deny()` calls `std::process::exit(2)` — those paths are
covered by integration tests in Task 10.

- [ ] **Step 2: Run tests**

```bash
cargo test -p coursers hook::pre
```

Expected: 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/coursers/src/hook/pre.rs
git commit -m "test(coursers): unit tests for hook::pre allow paths"
```

---

## Task 9: Unit tests for `hook::post` in `crates/coursers`

**Files:**

- Modify: `crates/coursers/src/hook/post.rs`

- [ ] **Step 1: Append tests to `crates/coursers/src/hook/post.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crs_core::loader::InMemoryRulesLoader;
    use crs_core::rules::{FailureLearning, RulesConfig};
    use crs_core::store::InMemoryStateStore;
    use super::super::{HookPayload, ToolInput};
    use serde_json::json;

    fn config_fl(enabled: bool) -> RulesConfig {
        RulesConfig {
            rules: vec![],
            failure_learning: FailureLearning {
                enabled,
                block_threshold: 3,
                window_seconds: 300,
                state_file: None,
                max_tracked_commands: 200,
                cleanup_after_seconds: 3600,
                message_template: None,
            },
        }
    }

    fn post_payload(cmd: &str, exit_code: i64) -> HookPayload {
        HookPayload {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(ToolInput { command: Some(cmd.to_string()) }),
            tool_response: Some(json!({ "exit_code": exit_code })),
        }
    }

    #[test]
    fn exit_zero_no_record() {
        let loader = InMemoryRulesLoader(config_fl(true));
        let store = InMemoryStateStore::new();
        run_with(&loader, &store, &post_payload("grep foo .", 0));
        assert!(store.get_state().failures.is_empty());
    }

    #[test]
    fn signal_exit_no_record() {
        let loader = InMemoryRulesLoader(config_fl(true));
        let store = InMemoryStateStore::new();
        run_with(&loader, &store, &post_payload("grep foo .", 130));
        assert!(store.get_state().failures.is_empty());
    }

    #[test]
    fn excluded_pattern_no_record() {
        let loader = InMemoryRulesLoader(config_fl(true));
        let store = InMemoryStateStore::new();
        run_with(&loader, &store, &post_payload("cmd 2>/dev/null", 1));
        assert!(store.get_state().failures.is_empty());
    }

    #[test]
    fn real_failure_recorded() {
        let loader = InMemoryRulesLoader(config_fl(true));
        let store = InMemoryStateStore::new();
        run_with(&loader, &store, &post_payload("grep foo .", 1));
        assert!(!store.get_state().failures.is_empty());
    }

    #[test]
    fn failure_learning_disabled_no_record() {
        let loader = InMemoryRulesLoader(config_fl(false));
        let store = InMemoryStateStore::new();
        run_with(&loader, &store, &post_payload("grep foo .", 1));
        assert!(store.get_state().failures.is_empty());
    }

    #[test]
    fn non_bash_tool_no_record() {
        let loader = InMemoryRulesLoader(config_fl(true));
        let store = InMemoryStateStore::new();
        let payload = HookPayload {
            tool_name: Some("Read".to_string()),
            tool_input: Some(ToolInput { command: Some("grep foo .".to_string()) }),
            tool_response: Some(json!({ "exit_code": 1 })),
        };
        run_with(&loader, &store, &payload);
        assert!(store.get_state().failures.is_empty());
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p coursers hook::post
```

Expected: 6 tests pass.

- [ ] **Step 3: Run the full unit test suite**

```bash
cargo test --workspace
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/coursers/src/hook/post.rs
git commit -m "test(coursers): unit tests for hook::post"
```

---

## Task 10: Integration test fixtures

**Files:**

- Create: `tests/integration/fixtures/rules_basic.json`
- Create: `tests/integration/fixtures/rules_empty.json`
- Create: `tests/integration/fixtures/payload_bash_grep.json`
- Create: `tests/integration/fixtures/payload_bash_ls.json`
- Create: `tests/integration/fixtures/payload_non_bash.json`
- Create: `tests/integration/fixtures/payload_post_fail.json`
- Create: `tests/integration/fixtures/payload_post_ok.json`
- Create: `tests/integration/fixtures/payload_post_signal.json`

- [ ] **Step 1: Create fixture directory and files**

`tests/integration/fixtures/rules_basic.json`:

```json
{
  "rules": [
    {
      "id": "no-grep",
      "pattern": "\\bgrep\\b",
      "exceptions": ["\\| grep"],
      "message": "Use the Grep tool instead of shell grep."
    },
    {
      "id": "no-cat",
      "pattern": "\\bcat\\s+[^|<]",
      "message": "Use the Read tool instead of cat."
    }
  ],
  "failure_learning": {
    "enabled": true,
    "block_threshold": 3,
    "window_seconds": 300
  }
}
```

`tests/integration/fixtures/rules_empty.json`:

```json
{
  "rules": [],
  "failure_learning": {
    "enabled": true,
    "block_threshold": 3,
    "window_seconds": 300
  }
}
```

`tests/integration/fixtures/payload_bash_grep.json`:

```json
{
  "tool_name": "Bash",
  "tool_input": { "command": "grep foo ." }
}
```

`tests/integration/fixtures/payload_bash_ls.json`:

```json
{
  "tool_name": "Bash",
  "tool_input": { "command": "ls -la" }
}
```

`tests/integration/fixtures/payload_non_bash.json`:

```json
{
  "tool_name": "Read",
  "tool_input": { "command": "grep foo ." }
}
```

`tests/integration/fixtures/payload_post_fail.json`:

```json
{
  "tool_name": "Bash",
  "tool_input": { "command": "grep foo ." },
  "tool_response": { "exit_code": 1 }
}
```

`tests/integration/fixtures/payload_post_ok.json`:

```json
{
  "tool_name": "Bash",
  "tool_input": { "command": "grep foo ." },
  "tool_response": { "exit_code": 0 }
}
```

`tests/integration/fixtures/payload_post_signal.json`:

```json
{
  "tool_name": "Bash",
  "tool_input": { "command": "grep foo ." },
  "tool_response": { "exit_code": 130 }
}
```

- [ ] **Step 2: Commit**

```bash
git add tests/
git commit -m "test(integration): add fixture JSON files"
```

---

## Task 11: Integration tests — `pre_hook` and `post_hook`

**Files:**

- Create: `tests/integration/pre_hook.rs`
- Create: `tests/integration/post_hook.rs`
- Create: `tests/integration/common.rs`
- Modify: `crates/coursers/Cargo.toml`

- [ ] **Step 1: Add integration test config to `crates/coursers/Cargo.toml`**

Append:

```toml
[dev-dependencies]
crs-core = { path = "../core", features = ["testing"] }
tempfile = { workspace = true }

[[test]]
name = "pre_hook"
path = "../../tests/integration/pre_hook.rs"

[[test]]
name = "post_hook"
path = "../../tests/integration/post_hook.rs"
```

Wait — integration tests that span the workspace root are better placed in a workspace-level
test harness. Instead, add a `[[test]]` to the **workspace root `Cargo.toml`** using a
separate package. The simplest approach for a workspace binary integration test is to place
the tests under the binary crate itself. Use `tests/` inside `crates/coursers/`:

Revise: integration tests live at `crates/coursers/tests/integration/`.

- [ ] **Step 2: Create `crates/coursers/tests/integration/common.rs`**

```rust
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn coursers_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_coursers"))
}

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/integration/fixtures")
        .join(name)
}

pub fn run_pre(payload_path: &Path, rules_path: &Path, state_path: &Path) -> Output {
    let payload = std::fs::read_to_string(payload_path).unwrap();
    Command::new(coursers_bin())
        .arg("pre")
        .env("COURSERS_RULES", rules_path)
        .env("COURSERS_STATE", state_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.as_mut().unwrap().write_all(payload.as_bytes()).unwrap();
            child.wait_with_output()
        })
        .unwrap()
}

pub fn run_post(payload_path: &Path, rules_path: &Path, state_path: &Path) -> Output {
    let payload = std::fs::read_to_string(payload_path).unwrap();
    Command::new(coursers_bin())
        .arg("post")
        .env("COURSERS_RULES", rules_path)
        .env("COURSERS_STATE", state_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.as_mut().unwrap().write_all(payload.as_bytes()).unwrap();
            child.wait_with_output()
        })
        .unwrap()
}
```

Note: `COURSERS_STATE` env var support needs to be added to `config.rs` — see Step 3.

- [ ] **Step 3: Add `COURSERS_STATE` env override to `crates/core/src/config.rs`**

```rust
use std::path::PathBuf;

pub fn rules_path() -> PathBuf {
    if let Ok(p) = std::env::var("COURSERS_RULES") {
        return PathBuf::from(p);
    }
    dirs::home_dir()
        .expect("home dir")
        .join(".claude/hooks/course-correct-rules.json")
}

pub fn state_path_default() -> PathBuf {
    if let Ok(p) = std::env::var("COURSERS_STATE") {
        return PathBuf::from(p);
    }
    dirs::home_dir()
        .expect("home dir")
        .join(".claude/hooks/course-correct-state.json")
}
```

- [ ] **Step 4: Create `crates/coursers/tests/integration/pre_hook.rs`**

```rust
mod common;
use common::{fixture, run_pre};
use tempfile::TempDir;

#[test]
fn blocked_command_exits_nonzero() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state.json");
    let out = run_pre(&fixture("payload_bash_grep.json"), &fixture("rules_basic.json"), &state);
    assert!(!out.status.success(), "expected non-zero exit");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("block") || stdout.contains("deny"), "stdout: {stdout}");
}

#[test]
fn allowed_command_exits_zero() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state.json");
    let out = run_pre(&fixture("payload_bash_ls.json"), &fixture("rules_basic.json"), &state);
    assert!(out.status.success(), "expected exit 0, got: {:?}", out.status);
}

#[test]
fn non_bash_passthrough() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state.json");
    let out = run_pre(&fixture("payload_non_bash.json"), &fixture("rules_basic.json"), &state);
    assert!(out.status.success(), "expected exit 0, got: {:?}", out.status);
}

#[test]
fn learned_failure_blocks_after_threshold() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state.json");
    let rules = fixture("rules_empty.json");

    // Record 3 failures via post
    for _ in 0..3 {
        run_post_inner(&fixture("payload_post_fail.json"), &rules, &state);
    }

    // Now pre should block the same command
    let out = run_pre(&fixture("payload_bash_grep.json"), &rules, &state);
    assert!(!out.status.success(), "expected block after 3 failures");
}

fn run_post_inner(payload: &std::path::Path, rules: &std::path::Path, state: &std::path::Path) {
    common::run_post(payload, rules, state);
}
```

- [ ] **Step 5: Create `crates/coursers/tests/integration/post_hook.rs`**

```rust
mod common;
use common::{fixture, run_post};
use tempfile::TempDir;

#[test]
fn failure_recorded_in_state() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state.json");
    run_post(&fixture("payload_post_fail.json"), &fixture("rules_basic.json"), &state);
    assert!(state.exists(), "state file should be created");
    let content = std::fs::read_to_string(&state).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(!parsed["failures"].as_object().unwrap().is_empty());
}

#[test]
fn success_not_recorded() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state.json");
    run_post(&fixture("payload_post_ok.json"), &fixture("rules_basic.json"), &state);
    assert!(!state.exists(), "state file should not be created for success");
}

#[test]
fn signal_not_recorded() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state.json");
    run_post(&fixture("payload_post_signal.json"), &fixture("rules_basic.json"), &state);
    assert!(!state.exists(), "state file should not be created for signal exit");
}

#[test]
fn excluded_pattern_not_recorded() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state.json");
    // Write an excluded-pattern payload inline
    let payload_path = tmp.path().join("payload_excluded.json");
    std::fs::write(&payload_path, r#"{
        "tool_name": "Bash",
        "tool_input": { "command": "cmd 2>/dev/null" },
        "tool_response": { "exit_code": 1 }
    }"#).unwrap();
    run_post(&payload_path, &fixture("rules_basic.json"), &state);
    assert!(!state.exists(), "excluded pattern should not be recorded");
}
```

- [ ] **Step 6: Add `tempfile` and `serde_json` to `crates/coursers` dev-dependencies**

In `crates/coursers/Cargo.toml`:

```toml
[dev-dependencies]
crs-core = { path = "../core", features = ["testing"] }
tempfile = { workspace = true }
serde_json = { workspace = true }
```

- [ ] **Step 7: Build the binary then run integration tests**

```bash
cargo build -p coursers
cargo test -p coursers --test pre_hook
cargo test -p coursers --test post_hook
```

Expected: all integration tests pass.

- [ ] **Step 8: Run the full suite**

```bash
cargo test --workspace
```

Expected: all tests pass.

- [ ] **Step 9: Commit**

```bash
git add crates/coursers/ tests/
git commit -m "test(integration): pre_hook and post_hook binary integration tests"
```

---

## Task 12: Smoke script

**Files:**

- Create: `scripts/smoke.nu`

- [ ] **Step 1: Create `scripts/` directory and write `smoke.nu`**

```nushell
#!/usr/bin/env nu
# smoke.nu — end-to-end smoke test for the coursers binary
# Usage: nu scripts/smoke.nu

let bin = (
    if (which coursers | length) > 0 {
        "coursers"
    } else if ("./target/release/coursers" | path exists) {
        "./target/release/coursers"
    } else {
        error make { msg: "coursers binary not found — run: cargo install --path crates/coursers" }
    }
)

let rules_path = ($env.HOME | path join ".claude/hooks/course-correct-rules.json")
if not ($rules_path | path exists) {
    error make { msg: $"Rules file not found: ($rules_path)" }
}

let tmp_dir = (mktemp -d)
let state_path = ($tmp_dir | path join "smoke-state.json")

def run_hook [subcommand: string, payload: string] {
    let result = (
        $payload
        | ^$bin $subcommand
            --env COURSERS_RULES=($rules_path)
            --env COURSERS_STATE=($state_path)
        | complete
    )
    $result
}

mut results = []

# Test 1: should-block payload
let block_payload = '{"tool_name":"Bash","tool_input":{"command":"grep foo ."}}'
let t1 = (do { $block_payload | ^$bin pre } | complete)
let t1_pass = ($t1.exit_code != 0) and (($t1.stdout | str contains "block") or ($t1.stdout | str contains "deny"))
$results = ($results | append { test: "should-block returns deny", pass: $t1_pass })

# Test 2: should-allow payload
let allow_payload = '{"tool_name":"Bash","tool_input":{"command":"ls -la"}}'
let t2 = (do { $allow_payload | ^$bin pre } | complete)
let t2_pass = ($t2.exit_code == 0)
$results = ($results | append { test: "should-allow exits 0", pass: $t2_pass })

# Test 3: post failure × 3 → pre blocks
let fail_payload = '{"tool_name":"Bash","tool_input":{"command":"smoke-test-unique-cmd-xyz"},"tool_response":{"exit_code":1}}'
let pre_payload = '{"tool_name":"Bash","tool_input":{"command":"smoke-test-unique-cmd-xyz"}}'
for _ in 1..3 {
    do { $fail_payload | ^$bin post } | complete | ignore
}
let t3 = (do { $pre_payload | ^$bin pre } | complete)
let t3_pass = ($t3.exit_code != 0)
$results = ($results | append { test: "learned failure blocks after threshold", pass: $t3_pass })

# Test 4: exit-0 post does not record
let ok_payload = '{"tool_name":"Bash","tool_input":{"command":"unique-ok-cmd-abc"},"tool_response":{"exit_code":0}}'
let state_before = if ($state_path | path exists) { open $state_path } else { {} }
do { $ok_payload | ^$bin post } | complete | ignore
let state_after = if ($state_path | path exists) { open $state_path } else { {} }
let t4_pass = not (($state_after | get -i failures | default {} | columns) | any { |k| $k == (^sha256sum <<< "unique-ok-cmd-abc" | str trim) })
$results = ($results | append { test: "exit-0 post does not record", pass: $t4_pass })

# Cleanup
rm -rf $tmp_dir

# Print results table
print ""
print "coursers smoke test results"
print "────────────────────────────────────────"
for r in $results {
    let icon = if $r.pass { "PASS" } else { "FAIL" }
    print $"  ($icon)  ($r.test)"
}
print "────────────────────────────────────────"
let failures = ($results | where pass == false | length)
if $failures > 0 {
    print $"  ($failures) test(s) failed"
    exit 1
} else {
    print "  all tests passed"
}
```

- [ ] **Step 2: Validate nu syntax**

```bash
nu -c 'nu --no-config-file scripts/smoke.nu --help' 2>&1 || true
nu -n -c 'source scripts/smoke.nu' 2>&1 | head -5
```

Expected: no syntax errors (the script will error on missing binary, which is fine for syntax check).

- [ ] **Step 3: Commit**

```bash
git add scripts/smoke.nu
git commit -m "feat: add smoke.nu end-to-end smoke test script"
```

---

## Task 13: Final verification

- [ ] **Step 1: Run the full test suite**

```bash
cargo test --workspace
```

Expected: all tests pass, no warnings.

- [ ] **Step 2: Run clippy**

```bash
cargo clippy --workspace -- -D warnings
```

Fix any warnings before proceeding.

- [ ] **Step 3: Verify smoke script syntax**

```bash
nu -n -c 'open scripts/smoke.nu | nu --stdin' 2>&1 || true
```

- [ ] **Step 4: Final commit if any clippy fixes were made**

```bash
git add -A
git commit -m "chore: clippy fixes after test harness implementation"
```

---

## Self-Review

**Spec coverage check:**

| Spec requirement                                             | Task      |
| ------------------------------------------------------------ | --------- |
| `RulesLoader` trait + `FsRulesLoader`                        | Task 2    |
| `StateStore` trait + `FsStateStore`                          | Task 3    |
| `InMemoryRulesLoader` + `InMemoryStateStore` (feature-gated) | Tasks 2+3 |
| `hook::pre::run_with` refactor                               | Task 4    |
| `hook::post::run_with` refactor                              | Task 4    |
| `rules.rs` unit tests (7 cases)                              | Task 5    |
| `state.rs` unit tests (8 cases)                              | Task 6    |
| `config.rs` unit tests (2 cases)                             | Task 7    |
| `hook::pre` unit tests                                       | Task 8    |
| `hook::post` unit tests                                      | Task 9    |
| Integration fixtures                                         | Task 10   |
| `pre_hook` integration tests (4 cases)                       | Task 11   |
| `post_hook` integration tests (4 cases)                      | Task 11   |
| `scripts/smoke.nu`                                           | Task 12   |
| Delete `hooks/` and `src/` dead code                         | Task 1    |

All spec requirements covered.

**Type consistency:** `HookPayload`, `ToolInput`, `run_with` signatures consistent across
Tasks 4, 8, 9, 11. `InMemoryStateStore::get_state()` defined in Task 3, used in Tasks 9.
`COURSERS_STATE` env var added in Task 11 Step 3, used by `common.rs` helper.

**`COURSERS_STATE` threading:** The `run_with` functions in `hook::pre` and `hook::post` use
`FsRulesLoader`/`FsStateStore` which call `state_path_default()`. That function now checks
`COURSERS_STATE` (Task 11 Step 3). Integration tests set this env var via `Command::env` so
state files land in the temp dir. This is correct.
