# Plan: Workspace Consolidation

## Goal

Extract `coursers-types` leaf crate, rename `crs-core` to
`coursers-core`, merge `crs` binary into `coursers` with symlink.

## Architecture

- Crates affected: all workspace members
- New crate: `coursers-types` (leaf, zero logic)
- Removed crate: `crs` (merged into `coursers`)
- Renamed crate: `crs-core` -> `coursers-core`
- Port traits gain `type Error` associated types

## Tech Stack

- Rust 2024 edition
- `serde`, `serde_json` (only deps for `coursers-types`)
- No new external dependencies

## Design Reference

`docs/designs/2026-06-28-workspace-consolidation-design.md`

---

## Tasks

### Task 1: Create `coursers-types` Cargo.toml and `lib.rs`

**Crate**: `coursers-types`
**File(s)**: `crates/types/Cargo.toml`, `crates/types/src/lib.rs`
**Run**: `cargo check -p coursers-types`

1. Create `crates/types/Cargo.toml`:

   ```toml
   [package]
   name = "coursers-types"
   version = "0.1.0"
   edition = "2024"
   license.workspace = true

   [dependencies]
   serde = { version = "1", features = ["derive"] }
   serde_json = "1"
   ```

2. Create `crates/types/src/lib.rs`:

   ```rust
   pub mod capture;
   pub mod config;
   pub mod filters;
   pub mod history;
   pub mod hook;
   pub mod obfsck;
   pub mod pipeline;
   pub mod ports;
   pub mod rtk;
   pub mod rules;
   pub mod state;
   pub mod stats;
   ```

3. Add `"crates/types"` to workspace members in root `Cargo.toml`.

4. Verify:

   ```
   cargo check -p coursers-types  → compiles
   ```

5. Do NOT commit yet -- commit after all types are populated.

---

### Task 2: Populate `types::config`

**Crate**: `coursers-types`
**File(s)**: `crates/types/src/config.rs`

1. Move from `crates/core/src/config.rs`:
   - `BYTES_PER_TOKEN`
   - `HookProtocol`
   - `ProfileConfig` (without `effective_state_path` method)

   ```rust
   use std::path::PathBuf;

   /// Approximate bytes per token (GPT/Claude tokenizer average).
   pub const BYTES_PER_TOKEN: usize = 4;

   /// Which hook protocol to use for output formatting and exit codes.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
   pub enum HookProtocol {
       /// Claude Code: exit 2 for deny.
       #[default]
       Claude,
       /// Codex: exit 0 + JSON `permissionDecision: "deny"`.
       Codex,
   }

   /// Resolved paths for a named profile (or the default profile).
   pub struct ProfileConfig {
       /// Path to the rules JSON file.
       pub rules_path: PathBuf,
       /// Path to the global (home-dir) state file.
       pub global_state_path: PathBuf,
       /// Project-local state path.
       pub local_state_path: PathBuf,
       /// Hook I/O protocol (Claude vs Codex).
       pub protocol: HookProtocol,
   }
   ```

---

### Task 3: Populate `types::hook`

**Crate**: `coursers-types`
**File(s)**: `crates/types/src/hook.rs`

1. Move from `crates/coursers/src/hook/mod.rs`:
   - `HookPayload`
   - `ToolInput`
   - `PreResponse`
   - `HookSpecificOutput`

   ```rust
   use serde::{Deserialize, Serialize};
   use serde_json::Value;

   /// Full Claude Code hook payload (PreToolUse or PostToolUse).
   #[derive(Debug, Deserialize)]
   pub struct HookPayload {
       pub tool_name: Option<String>,
       pub tool_input: Option<ToolInput>,
       pub tool_response: Option<Value>,
       pub session_id: Option<String>,
       pub cwd: Option<String>,
   }

   /// The `tool_input` field of a hook payload.
   #[derive(Debug, Deserialize)]
   pub struct ToolInput {
       pub command: Option<String>,
   }

   /// PreToolUse response envelope.
   #[derive(Debug, Serialize)]
   pub struct PreResponse {
       #[serde(rename = "hookSpecificOutput")]
       pub hook_specific_output: HookSpecificOutput,
   }

   /// Inner payload of a `PreToolUse` permission response.
   #[derive(Debug, Serialize)]
   pub struct HookSpecificOutput {
       #[serde(rename = "hookEventName")]
       pub hook_event_name: String,
       #[serde(rename = "permissionDecision")]
       pub permission_decision: String,
       #[serde(rename = "permissionDecisionReason")]
       pub permission_decision_reason: String,
   }
   ```

---

### Task 4: Populate `types::rules`

**Crate**: `coursers-types`
**File(s)**: `crates/types/src/rules.rs`

1. Move from `crates/core/src/rules.rs`:
   - `Rule`
   - `FailureLearning`
   - `RulesConfig`

   ```rust
   use serde::{Deserialize, Serialize};

   /// A single course-correction rule.
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct Rule {
       pub id: String,
       #[serde(default = "default_true")]
       pub enabled: bool,
       pub pattern: String,
       #[serde(default)]
       pub pattern_flags: String,
       #[serde(default)]
       pub exceptions: Vec<String>,
       #[serde(default)]
       pub target_commands: Vec<String>,
       pub message: Option<String>,
   }

   fn default_true() -> bool {
       true
   }

   /// Configuration for the failure-learning subsystem.
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct FailureLearning {
       #[serde(default)]
       pub enabled: bool,
       #[serde(default = "default_threshold")]
       pub block_threshold: usize,
       #[serde(default = "default_window")]
       pub window_seconds: u64,
       pub state_file: Option<String>,
       #[serde(default = "default_max_tracked")]
       pub max_tracked_commands: usize,
       #[serde(default = "default_cleanup")]
       pub cleanup_after_seconds: u64,
       pub message_template: Option<String>,
   }

   fn default_threshold() -> usize { 3 }
   fn default_window() -> u64 { 300 }
   fn default_max_tracked() -> usize { 200 }
   fn default_cleanup() -> u64 { 3600 }

   impl Default for FailureLearning {
       fn default() -> Self {
           Self {
               enabled: false,
               block_threshold: default_threshold(),
               window_seconds: default_window(),
               state_file: None,
               max_tracked_commands: default_max_tracked(),
               cleanup_after_seconds: default_cleanup(),
               message_template: None,
           }
       }
   }

   /// Root rules configuration.
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct RulesConfig {
       pub rules: Vec<Rule>,
       #[serde(default)]
       pub failure_learning: FailureLearning,
   }
   ```

2. Verify serde defaults match existing `crates/core/src/rules.rs`
   exactly before writing.

---

### Task 5: Populate `types::state`

**Crate**: `coursers-types`
**File(s)**: `crates/types/src/state.rs`

1. Move from `crates/core/src/state.rs`:
   - `FailureEntry`
   - `State`

   ```rust
   use serde::{Deserialize, Serialize};
   use std::collections::HashMap;

   /// A single command's failure history.
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct FailureEntry {
       pub command_preview: String,
       pub timestamps: Vec<u64>,
       pub last_seen: f64,
   }

   /// Root failure-learning state.
   #[derive(Debug, Clone, Serialize, Deserialize, Default)]
   pub struct State {
       pub failures: HashMap<String, FailureEntry>,
   }
   ```

---

### Task 6: Populate `types::capture`

**Crate**: `coursers-types`
**File(s)**: `crates/types/src/capture.rs`

1. Move from `crates/core/src/analyze/capture.rs`:
   - `SuggestionRecord`
   - `SuggestionParams`
   - `DedupeKey`

   Read the source file to get exact field definitions and derives
   before writing. Preserve `SuggestionRecord::new()` constructor
   if it contains no I/O.

---

### Task 7: Populate `types::filters`

**Crate**: `coursers-types`
**File(s)**: `crates/types/src/filters.rs`

1. Move from `crates/core/src/hook/filters.rs`:
   - `FilterMode`
   - `FilterRule` (with `default_max_lines` helper)
   - `FiltersConfig` (without `load_from` method)

2. Move from `crates/core/src/hook/tool_swap.rs`:
   - `ToolSwapConfig` (with `Default` impl)

3. Move from `crates/core/src/hook/rewrite.rs`:
   - `RewriteRule`
   - `RewriteConfig`

4. Move from `crates/crs/src/lib.rs`:
   - `FilterResult`
   - `FilterPayload`

---

### Task 8: Populate `types::stats`

**Crate**: `coursers-types`
**File(s)**: `crates/types/src/stats.rs`

1. Move from `crates/core/src/analyze/stats.rs`:
   - `Stats`

   ```rust
   use serde::{Deserialize, Serialize};
   use std::collections::HashMap;

   /// Cumulative block statistics per rule id.
   #[derive(Debug, Clone, Serialize, Deserialize, Default)]
   pub struct Stats {
       pub blocks: HashMap<String, u64>,
       pub last_seen: HashMap<String, f64>,
   }
   ```

---

### Task 9: Populate `types::history`

**Crate**: `coursers-types`
**File(s)**: `crates/types/src/history.rs`

1. Move from `crates/core/src/analyze/history.rs`:
   - `CommandRecord`
   - `DiscoverOpts` (with `Default` impl)

---

### Task 10: Populate `types::obfsck`

**Crate**: `coursers-types`
**File(s)**: `crates/types/src/obfsck.rs`

1. Move from `crates/core/src/obfsck.rs`:
   - `AuditHit`
   - `FilterSuggestion`

---

### Task 11: Populate `types::rtk`

**Crate**: `coursers-types`
**File(s)**: `crates/types/src/rtk.rs`

1. Move from `crates/core/src/rtk.rs`:
   - All 10 domain structs (`RtkDiscoverReport`, `RtkSupportedEntry`,
     `RtkUnsupportedEntry`, `RtkGainReport`, `RtkGainEntry`,
     `RtkSessionEntry`, `RtkVerifyResult`, `RtkHookAudit`,
     `RtkAuditEntry`, `RtkProbeResult`)

   Copy exact field definitions from source.

---

### Task 12: Populate `types::pipeline`

**Crate**: `coursers-types`
**File(s)**: `crates/types/src/pipeline.rs`

1. Move from `crates/core/src/hook/pipeline.rs`:
   - `HookEvent`
   - `When`
   - `HookAction`
   - `HookRule`
   - `HookPipelineConfig` (without `load_from` method)
   - `HookContext`
   - `PipelineResult`
   - `HookDiagnostic`
   - `DiagLevel`

   Read exact field definitions from source before writing. Exclude
   any methods that do file I/O (like `HookPipelineConfig::load_from`).

---

### Task 13: Populate `types::ports`

**Crate**: `coursers-types`
**File(s)**: `crates/types/src/ports.rs`

1. Define all port traits with associated error types:

   ```rust
   use crate::capture::SuggestionRecord;
   use crate::filters::FiltersConfig;
   use crate::history::CommandRecord;
   use crate::rules::RulesConfig;
   use crate::state::State;
   use crate::stats::Stats;
   use crate::obfsck::{AuditHit, FilterSuggestion};
   use crate::rtk::*;
   use std::path::PathBuf;

   pub trait RulesLoader {
       type Error: std::fmt::Debug;
       fn load(&self) -> Result<RulesConfig, Self::Error>;
   }

   pub trait StateStore {
       type Error: std::fmt::Debug;
       fn load(&self) -> Result<State, Self::Error>;
       fn save(&self, state: &State) -> Result<(), Self::Error>;
   }

   pub trait CaptureStore {
       type Error: std::fmt::Debug;
       fn record(
           &self, record: SuggestionRecord,
       ) -> Result<(), Self::Error>;
       fn mark_accepted(
           &self, session_id: &str, command: &str, exit_code: i64,
       ) -> Result<(), Self::Error>;
   }

   pub trait CommandSource {
       fn commands(&self) -> impl Iterator<Item = CommandRecord>;
   }

   pub trait StatsStore {
       type Error: std::fmt::Debug;
       fn load(&self) -> Result<Stats, Self::Error>;
       fn save(&self, stats: &Stats) -> Result<(), Self::Error>;
   }

   pub trait FiltersLoader {
       type Error: std::fmt::Debug;
       fn load(&self) -> Result<FiltersConfig, Self::Error>;
       fn filters_path(&self) -> Option<PathBuf>;
   }

   pub trait ObfsckMcp {
       fn audit(&self, text: &str) -> Vec<AuditHit>;
       fn generate_filters(
           &self, examples: &[String],
       ) -> Vec<FilterSuggestion>;
   }

   pub trait RtkAnalysis {
       fn discover(
           &self, since_days: u32,
       ) -> Option<RtkDiscoverReport>;
       fn gain(&self) -> Option<RtkGainReport>;
       fn session(&self) -> Option<Vec<RtkSessionEntry>>;
       fn verify(&self) -> Option<RtkVerifyResult>;
       fn hook_audit(&self) -> Option<RtkHookAudit>;
       fn version(&self) -> Option<String>;
   }

   pub trait RtkRewrite {
       fn rewrite(&self, command: &str) -> Option<String>;
       fn probe(&self, command: &str) -> Option<RtkProbeResult>;
       fn check(&self, command: &str) -> bool;
       fn list_proxies(&self) -> Vec<String>;
       fn flush(&self) -> bool;
   }

   pub trait VarExpander {
       fn expand(&self, command: &str) -> String;
   }

   pub trait FileInfo {
       fn file_size(&self, path: &str) -> Option<u64>;
       fn count_lines(&self, path: &str) -> Option<usize>;
       fn avg_bytes_per_line(&self, path: &str) -> Option<usize>;
   }
   ```

---

### Task 14: Compile `coursers-types` and commit

**Crate**: `coursers-types`
**Run**: `cargo check -p coursers-types`

1. Verify:

   ```
   cargo check -p coursers-types  → compiles
   ```

2. Do NOT update consumers yet. This commit only adds the new crate.

3. Run: `git branch --show-current`
   Verify on correct branch.
   Commit: `git commit -m "feat: create coursers-types leaf crate with domain types and port traits"`

---

### Task 15: Add `coursers-types` dep to `crs-core` and re-export

**Crate**: `crs-core`
**File(s)**: `crates/core/Cargo.toml`, `crates/core/src/lib.rs`,
and each module that defined a moved type.

1. Add to `crates/core/Cargo.toml`:

   ```toml
   coursers-types = { path = "../types" }
   ```

2. In each module that previously defined a type now in
   `coursers-types`, replace the struct/enum definition with a
   re-export:

   ```rust
   // crates/core/src/config.rs
   pub use coursers_types::config::{
       BYTES_PER_TOKEN, HookProtocol, ProfileConfig,
   };
   ```

   Repeat for: `rules.rs`, `state.rs`, `analyze/capture.rs`,
   `hook/filters.rs`, `hook/rewrite.rs`, `hook/tool_swap.rs`,
   `analyze/stats.rs`, `analyze/history.rs`, `obfsck.rs`, `rtk.rs`,
   `hook/pipeline.rs`.

3. Re-export port traits from their original modules:

   ```rust
   // crates/core/src/loader.rs
   pub use coursers_types::ports::RulesLoader;
   ```

   Repeat for `store.rs`, `analyze/capture.rs`, `analyze/stats.rs`,
   `analyze/history.rs`, `obfsck.rs`, `rtk.rs`, `hook/filters.rs`,
   `hook/tool_swap.rs`, `parse/expand.rs`.

4. Update trait impls to use `type Error = CourserError;` where the
   trait now requires an associated error type. Example:

   ```rust
   impl RulesLoader for FsRulesLoader {
       type Error = CourserError;
       fn load(&self) -> Result<RulesConfig, Self::Error> {
           Ok(fs_load())
       }
   }
   ```

   Apply to every adapter and test double that implements a port
   trait.

5. Move `effective_state_path` from `ProfileConfig` impl block to a
   free function in `crates/core/src/config.rs`:

   ```rust
   /// Returns project-local state path if it exists, else global.
   pub fn effective_state_path(cfg: &ProfileConfig) -> &PathBuf {
       if cfg.local_state_path.exists() {
           &cfg.local_state_path
       } else {
           &cfg.global_state_path
       }
   }
   ```

   Update all callers of `cfg.effective_state_path()` to
   `effective_state_path(&cfg)`.

6. Verify:

   ```
   cargo test --workspace  → all green
   cargo clippy --workspace -- -D warnings  → clean
   ```

7. Commit: `git commit -m "refactor: re-export types from coursers-types in crs-core"`

---

### Task 16: Update `coursers` binary to use `coursers-types`

**Crate**: `coursers`
**File(s)**: `crates/coursers/Cargo.toml`,
`crates/coursers/src/hook/mod.rs`

1. Add to `crates/coursers/Cargo.toml`:

   ```toml
   coursers-types = { path = "../types" }
   ```

2. In `crates/coursers/src/hook/mod.rs`, replace local struct
   definitions (`HookPayload`, `ToolInput`, `PreResponse`,
   `HookSpecificOutput`) with re-exports from `coursers_types::hook`.

3. Update all `use` paths in `pre.rs`, `post.rs`, `mod.rs` to import
   from `coursers_types` where needed.

4. Verify:

   ```
   cargo test --workspace  → all green
   cargo clippy --workspace -- -D warnings  → clean
   ```

5. Commit: `git commit -m "refactor: coursers binary imports types from coursers-types"`

---

### Task 17: Rename `crs-core` to `coursers-core`

**Crate**: `coursers-core` (rename)
**File(s)**: `crates/core/Cargo.toml`, all files with `use crs_core::`

1. In `crates/core/Cargo.toml`, change:

   ```toml
   name = "coursers-core"
   ```

2. In all other `Cargo.toml` files that depend on `crs-core`, update:

   ```toml
   # before
   crs-core = { path = "../core" }
   # after
   coursers-core = { path = "../core" }
   ```

3. Global find-and-replace across the workspace:
   - `use crs_core::` -> `use coursers_core::`
   - `crs_core::` -> `coursers_core::` (in non-use positions)
   - `extern crate crs_core` -> `extern crate coursers_core`

4. Verify:

   ```
   cargo test --workspace  → all green
   cargo clippy --workspace -- -D warnings  → clean
   ```

5. Commit: `git commit -m "refactor: rename crs-core to coursers-core"`

---

### Task 18: Move filter logic to `coursers-core`

**Crate**: `coursers-core`, `coursers-types`
**File(s)**: `crates/core/src/filters.rs` (new or extend existing),
`crates/crs/src/lib.rs`

1. `FilterResult` and `FilterPayload` are already in
   `coursers-types` (Task 7). Verify they compile there.

2. Move `run_filter` and `apply_filter` from `crates/crs/src/lib.rs`
   into a new module in `coursers-core` (e.g.,
   `crates/core/src/filter_logic.rs`).

3. Update `crates/crs/src/main.rs` to import from `coursers_core`
   instead of `crs_lib`.

4. Remove `run_filter`, `apply_filter`, `FilterResult`,
   `FilterPayload` from `crates/crs/src/lib.rs`.

5. Verify:

   ```
   cargo test --workspace  → all green
   ```

6. Commit: `git commit -m "refactor: move filter logic to coursers-core"`

---

### Task 19: Move `jsonl_source` to `coursers-core`

**Crate**: `coursers-core`
**File(s)**: `crates/core/src/jsonl_source.rs` (new),
`crates/crs/src/jsonl_source.rs` (remove)

1. Move `crates/crs/src/jsonl_source.rs` to
   `crates/core/src/jsonl_source.rs`.

2. Add `pub mod jsonl_source;` to `crates/core/src/lib.rs`.

3. Update imports in `crates/crs/src/main.rs` to use
   `coursers_core::jsonl_source`.

4. Remove `pub mod jsonl_source;` from `crates/crs/src/lib.rs`.

5. Verify:

   ```
   cargo test --workspace  → all green
   ```

6. Commit: `git commit -m "refactor: move jsonl_source adapter to coursers-core"`

---

### Task 20: Move binary-local adapters to `coursers`

**Crate**: `coursers`
**File(s)**: `crates/coursers/src/nu_check/` (new),
`crates/coursers/src/obfsck/` (new),
`crates/coursers/src/rtk/` (new)

1. Copy `crates/crs/src/nu_check/` to `crates/coursers/src/nu_check/`.
2. Copy `crates/crs/src/obfsck/` to `crates/coursers/src/obfsck/`.
3. Copy `crates/crs/src/rtk/` to `crates/coursers/src/rtk/`.
4. Add `mod nu_check; mod obfsck; mod rtk;` to appropriate location
   in `crates/coursers/src/` (may need a new module file or add to
   `main.rs`).
5. Update internal imports in copied modules to use `coursers_core`
   and `coursers_types`.
6. Remove these modules from `crates/crs/src/lib.rs`.
7. Delete `crates/crs/src/lib.rs` if now empty (only
   `run_rewrite` should remain; move it inline to `main.rs` if so).

8. Verify:

   ```
   cargo test --workspace  → all green
   ```

9. Commit: `git commit -m "refactor: move nu_check, obfsck, rtk adapters to coursers binary"`

---

### Task 21: Merge `crs` subcommands into `coursers`

**Crate**: `coursers`
**File(s)**: `crates/coursers/src/main.rs`

1. Read `crates/crs/src/main.rs` fully. Copy:
   - All `Command` enum variants (except `Filter`, `Rewrite` which
     are already subcommands of the hook system -- reconcile)
   - All `cmd_*` functions
   - All helper functions (`read_stdin_payload`, `event_str_for`,
     `emit_tool_swap`, `emit_system_message`, `emit_message`,
     `emit_rewrite`, `write_stdout`, `load_rewrite_config`,
     `check_probe_match`, `handle_probe_result`,
     `handle_bare_failure`, etc.)

2. Merge the `Command` enum: combine `coursers` Pre/Post with all
   `crs` variants into one enum.

3. Wire the `main()` dispatch to handle all variants.

4. Add any missing dependencies to `crates/coursers/Cargo.toml`
   (e.g., `toml`, `clap`, `dirs`, `regex`, `redb`, `prefixe`).

5. Verify:

   ```
   cargo test --workspace  → all green
   cargo clippy --workspace -- -D warnings  → clean
   ```

6. Commit: `git commit -m "feat: merge crs subcommands into coursers binary"`

---

### Task 22: Add `crs` symlink and rename e2e

**File(s)**: root `Cargo.toml`, `crates/e2e/Cargo.toml`,
`crates/coursers/Cargo.toml`

1. In `crates/coursers/Cargo.toml`, add a second binary target:

   ```toml
   [[bin]]
   name = "coursers"
   path = "src/main.rs"

   [[bin]]
   name = "crs"
   path = "src/main.rs"
   ```

   This makes `cargo install` produce both `coursers` and `crs`
   binaries (identical, not a symlink, but functionally equivalent).

2. In `crates/e2e/Cargo.toml`, rename:

   ```toml
   name = "coursers-e2e"
   ```

3. Update workspace members in root `Cargo.toml` if needed.

4. Verify:

   ```
   cargo build --workspace  → both binaries built
   cargo test --workspace  → all green
   ```

5. Commit: `git commit -m "feat: add crs binary alias, rename e2e crate"`

---

### Task 23: Remove `crates/crs/`

**File(s)**: `crates/crs/` (delete), root `Cargo.toml`

1. Remove `"crates/crs"` from workspace members in root
   `Cargo.toml`.

2. Delete `crates/crs/` directory.

3. Move any integration tests from `crates/crs/tests/` into
   `crates/coursers/tests/` or `crates/e2e/tests/`, updating
   imports.

4. Verify:

   ```
   cargo test --workspace  → all green
   cargo clippy --workspace -- -D warnings  → clean
   ```

5. Commit: `git commit -m "chore: remove crates/crs after merge into coursers"`

---

### Task 24: TDD — Integration test for Codex deny exit code

**Crate**: `coursers`
**File(s)**: `crates/coursers/tests/pre_hook.rs`
**Run**: `cargo nextest run -p coursers -- codex_deny_exits_0`

1. Write failing test:

   ```rust
   /// Codex protocol: deny must exit 0 (not 2).
   #[test]
   fn codex_deny_exits_0() {
       use std::process::Command;

       // Feed a grep command (blocked by no-grep-use-tool rule)
       // through coursers pre --profile codex.
       let output = Command::new(env!("CARGO_BIN_EXE_coursers"))
           .args(["pre", "--profile", "codex"])
           .env(
               "COURSERS_RULES",
               env!("CARGO_MANIFEST_DIR").to_string()
                   + "/tests/fixtures/rules-with-grep.json",
           )
           .write_stdin(
               r#"{"tool_name":"Bash","tool_input":{"command":"grep foo ."}}"#,
           )
           .output()
           .expect("failed to run coursers pre");

       // Codex protocol: exit 0, not 2
       assert_eq!(
           output.status.code(),
           Some(0),
           "Codex deny must exit 0, got {:?}\nstderr: {}",
           output.status.code(),
           String::from_utf8_lossy(&output.stderr),
       );

       let stdout = String::from_utf8_lossy(&output.stdout);
       let v: serde_json::Value =
           serde_json::from_str(&stdout).expect("stdout must be JSON");
       assert_eq!(
           v["hookSpecificOutput"]["permissionDecision"], "deny",
           "response must contain deny decision",
       );
   }
   ```

   Note: `write_stdin` requires the `assert_cmd` crate or piping
   via `std::process::Stdio`. Adjust to match existing test patterns
   in `crates/coursers/tests/pre_hook.rs`. Read that file first.

2. Create fixture `crates/coursers/tests/fixtures/rules-with-grep.json`:

   ```json
   {
     "rules": [
       {
         "id": "no-grep-use-tool",
         "enabled": true,
         "pattern": "\\bgrep\\b",
         "message": "Use the Grep tool instead"
       }
     ],
     "failure_learning": { "enabled": false }
   }
   ```

3. Run: `cargo nextest run -p coursers -- codex_deny_exits_0`
   Expected: FAIL (test doesn't exist yet or fixture missing)

4. Verify the existing `coursers pre --profile codex` code path
   already handles this correctly (it should, from the earlier
   multi-CLI hook work). If it does, the test passes immediately
   after writing it -- that's fine, it's a regression test for the
   protocol contract.

5. Write the Claude counterpart:

   ```rust
   /// Claude protocol: deny must exit 2.
   #[test]
   fn claude_deny_exits_2() {
       use std::process::Command;

       let output = Command::new(env!("CARGO_BIN_EXE_coursers"))
           .args(["pre"])
           .env(
               "COURSERS_RULES",
               env!("CARGO_MANIFEST_DIR").to_string()
                   + "/tests/fixtures/rules-with-grep.json",
           )
           .write_stdin(
               r#"{"tool_name":"Bash","tool_input":{"command":"grep foo ."}}"#,
           )
           .output()
           .expect("failed to run coursers pre");

       assert_eq!(
           output.status.code(),
           Some(2),
           "Claude deny must exit 2",
       );
   }
   ```

6. Verify:

   ```
   cargo nextest run -p coursers -- deny_exits  → both green
   ```

7. Commit: `git commit -m "test: integration tests for Claude/Codex deny exit codes"`

---

### Task 25: TDD — Unit test for validate-codex-hooks logic

**Crate**: `coursers` (after merge) or `coursers-core`
**File(s)**: `crates/coursers/src/validate.rs` (extract),
or inline test in `main.rs`
**Run**: `cargo nextest run -p coursers -- codex_hooks`

1. Extract the core checking logic from `cmd_validate_codex_hooks`
   into a testable pure function:

   ```rust
   /// Check a hooks JSON string for expected Codex hook commands.
   /// Returns the list of missing command strings.
   pub fn check_codex_hooks(json_content: &str) -> Vec<&'static str> {
       const EXPECTED: &[&str] = &[
           "coursers pre --profile codex",
           "crs rewrite --profile codex",
           "coursers post --profile codex",
           "crs filter --profile codex",
       ];
       EXPECTED
           .iter()
           .filter(|cmd| !json_content.contains(*cmd))
           .copied()
           .collect()
   }
   ```

2. Write failing test first:

   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn codex_hooks_all_present() {
           let json = r#"{
               "hooks": [
                   {"command": "coursers pre --profile codex"},
                   {"command": "crs rewrite --profile codex"},
                   {"command": "coursers post --profile codex"},
                   {"command": "crs filter --profile codex"}
               ]
           }"#;
           assert!(
               check_codex_hooks(json).is_empty(),
               "all 4 commands present, none missing",
           );
       }

       #[test]
       fn codex_hooks_missing_two() {
           let json = r#"{
               "hooks": [
                   {"command": "coursers pre --profile codex"},
                   {"command": "coursers post --profile codex"}
               ]
           }"#;
           let missing = check_codex_hooks(json);
           assert_eq!(missing.len(), 2);
           assert!(missing.contains(&"crs rewrite --profile codex"));
           assert!(missing.contains(&"crs filter --profile codex"));
       }

       #[test]
       fn codex_hooks_empty_json() {
           let missing = check_codex_hooks("{}");
           assert_eq!(missing.len(), 4);
       }
   }
   ```

3. Run: `cargo nextest run -p coursers -- codex_hooks`
   Expected: FAIL (function doesn't exist yet)

4. Implement `check_codex_hooks` as shown in step 1.

5. Update `cmd_validate_codex_hooks` to call `check_codex_hooks`
   instead of inline string matching.

6. Run: `cargo nextest run -p coursers -- codex_hooks`
   Expected: all 3 green

7. Verify:

   ```
   cargo nextest run -p coursers  → all green
   cargo clippy -p coursers -- -D warnings  → clean
   ```

8. Commit: `git commit -m "test: unit tests for validate-codex-hooks logic"`

---

### Task 26: TDD — Property test for `extract_output`

**Crate**: `coursers-core`
**File(s)**: `crates/core/src/hook/protocol.rs`
**Run**: `cargo nextest run -p coursers-core -- extract_output`

1. Add `proptest` to `crates/core/Cargo.toml` dev-dependencies if
   not already present:

   ```toml
   [dev-dependencies]
   proptest = "1"
   ```

2. Write failing property test (append to existing `mod tests` in
   `protocol.rs`):

   ```rust
   #[cfg(test)]
   mod proptests {
       use super::*;
       use proptest::prelude::*;

       proptest! {
           /// Invariant: extract_output concatenates stdout before
           /// output for any string pair.
           #[test]
           fn extract_output_concatenation_order(
               stdout in ".*",
               output in ".*",
           ) {
               let v = serde_json::json!({
                   "stdout": stdout,
                   "output": output,
               });
               let result = extract_output(&v);
               if stdout.is_empty() && output.is_empty() {
                   prop_assert!(result.is_none());
               } else {
                   let combined = result.unwrap();
                   prop_assert_eq!(
                       combined,
                       format!("{stdout}{output}"),
                   );
               }
           }

           /// Invariant: extract_output returns None iff both fields
           /// are absent or empty.
           #[test]
           fn extract_output_none_iff_both_empty(
               has_stdout in proptest::bool::ANY,
               has_output in proptest::bool::ANY,
               stdout_val in ".*",
               output_val in ".*",
           ) {
               let mut map = serde_json::Map::new();
               if has_stdout {
                   map.insert(
                       "stdout".into(),
                       serde_json::Value::String(
                           stdout_val.clone(),
                       ),
                   );
               }
               if has_output {
                   map.insert(
                       "output".into(),
                       serde_json::Value::String(
                           output_val.clone(),
                       ),
                   );
               }
               let v = serde_json::Value::Object(map);
               let result = extract_output(&v);

               let effective_stdout = if has_stdout {
                   &stdout_val
               } else {
                   ""
               };
               let effective_output = if has_output {
                   &output_val
               } else {
                   ""
               };

               if effective_stdout.is_empty()
                   && effective_output.is_empty()
               {
                   prop_assert!(result.is_none());
               } else {
                   prop_assert_eq!(
                       result.unwrap(),
                       format!(
                           "{effective_stdout}{effective_output}"
                       ),
                   );
               }
           }
       }
   }
   ```

3. Run: `cargo nextest run -p coursers-core -- extract_output`
   Expected: FAIL (proptest not in deps or tests don't exist)

4. Add the dependency and tests.

5. Run: `cargo nextest run -p coursers-core -- extract_output`
   Expected: all green (including existing unit tests + new
   property tests)

6. Commit `proptest-regressions/` if any counterexamples found.

7. Verify:

   ```
   cargo nextest run -p coursers-core  → all green
   cargo clippy -p coursers-core -- -D warnings  → clean
   ```

8. Commit: `git commit -m "test: property tests for extract_output concatenation invariant"`

---

## Verification

After all tasks:

```
cargo test --workspace            → all green
cargo clippy --workspace -- -D warnings  → clean
cargo install --path crates/coursers     → installs both coursers and crs
which coursers && which crs              → both on PATH
echo '{}' | coursers filter              → runs (passthrough)
echo '{}' | crs filter                   → identical behavior
```

## Risk

- [x] Breaking API: all `use crs_core::` paths change; port traits
      gain `type Error`. Mitigated by re-exports (Task 15) before rename
      (Task 17).
- [ ] New external dependency: none
- [ ] Feature flag required: no
- [x] Large diff: Tasks 15, 17, 21 are high line-count. Each is
      mechanical but review carefully.
