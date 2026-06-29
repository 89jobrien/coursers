# Design: Workspace Consolidation

## Goal

Extract a leaf `coursers-types` crate for all domain types and port
traits, rename `crs-core` to `coursers-core`, and merge the `crs`
binary into `coursers` with a `crs` symlink for backwards compatibility.

## Approved Approach

Option C from brainstorm: full type/trait extraction into a leaf crate,
binary merge with symlink, associated error types on all port traits.

## Crate Ownership

| Directory          | Name             | Role                             |
| ------------------ | ---------------- | -------------------------------- |
| `crates/types/`    | `coursers-types` | Leaf: domain types + port traits |
| `crates/core/`     | `coursers-core`  | Logic + adapters + error types   |
| `crates/coursers/` | `coursers`       | Merged binary (all subcommands)  |
| `crates/e2e/`      | `coursers-e2e`   | End-to-end tests                 |
| `crates/xtask/`    | `xtask`          | Build tasks (unchanged)          |

**Removed**: `crates/crs/` (merged into `coursers`)

## Dependencies

```
coursers-types       serde, serde_json, std only
    ^       ^
    |       |
coursers-core       coursers-types + dirs, regex, toml, redb, tempfile, prefixe
    ^       |
    |       |
 coursers  coursers-e2e
```

## Public API: `coursers-types`

### Module: `types::config`

```rust
pub const BYTES_PER_TOKEN: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HookProtocol {
    #[default]
    Claude,
    Codex,
}

pub struct ProfileConfig {
    pub rules_path: PathBuf,
    pub global_state_path: PathBuf,
    pub local_state_path: PathBuf,
    pub protocol: HookProtocol,
}
```

### Module: `types::hook`

```rust
#[derive(Debug, Deserialize)]
pub struct HookPayload {
    pub tool_name: Option<String>,
    pub tool_input: Option<ToolInput>,
    pub tool_response: Option<serde_json::Value>,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ToolInput {
    pub command: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PreResponse {
    pub hook_specific_output: HookSpecificOutput,
}

#[derive(Debug, Serialize)]
pub struct HookSpecificOutput {
    pub hook_event_name: String,
    pub permission_decision: String,
    pub permission_decision_reason: String,
}
```

### Module: `types::rules`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub enabled: bool,
    pub pattern: String,
    pub pattern_flags: String,
    pub exceptions: Vec<String>,
    pub target_commands: Vec<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FailureLearning {
    pub enabled: bool,
    pub block_threshold: usize,
    pub window_seconds: u64,
    pub state_file: Option<String>,
    pub max_tracked_commands: usize,
    pub cleanup_after_seconds: u64,
    pub message_template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesConfig {
    pub rules: Vec<Rule>,
    pub failure_learning: FailureLearning,
}
```

### Module: `types::state`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureEntry {
    pub command_preview: String,
    pub timestamps: Vec<u64>,
    pub last_seen: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
    pub failures: HashMap<String, FailureEntry>,
}
```

### Module: `types::capture`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionRecord {
    pub command: String,
    pub suggestion: String,
    pub rule_id: String,
    pub cwd: String,
    pub session_id: Option<String>,
    pub tool_name: String,
    pub timestamp: f64,
    pub count: u64,
    pub accepted: bool,
}

pub struct SuggestionParams {
    pub command: String,
    pub suggestion: String,
    pub rule_id: String,
    pub cwd: String,
    pub session_id: Option<String>,
    pub tool_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DedupeKey {
    pub command: String,
    pub rule_id: String,
}
```

### Module: `types::filters`

```rust
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FilterMode {
    #[default]
    Passthrough,
    FailuresOnly,
    ErrorsOnly,
    Truncate,
    MatchLines,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FilterRule {
    pub pattern: String,
    pub mode: FilterMode,
    pub max_lines: usize,
    pub match_pattern: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FiltersConfig {
    pub filters: Vec<FilterRule>,
    pub tool_swap: ToolSwapConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolSwapConfig {
    pub cat_token_limit: usize,
    pub tail_limit_max: usize,
    pub find_depth_max: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RewriteRule {
    pub pattern: String,
    pub replace: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RewriteConfig {
    pub rewrites: Vec<RewriteRule>,
}

#[derive(Debug, PartialEq)]
pub enum FilterResult {
    Passthrough,
    Replace(String),
    Suppress,
}

pub struct FilterPayload {
    pub command: String,
    pub output: String,
    pub exit_code: i64,
}
```

### Module: `types::stats`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Stats {
    pub blocks: HashMap<String, u64>,
    pub last_seen: HashMap<String, f64>,
}
```

### Module: `types::history`

```rust
pub struct CommandRecord {
    pub command: String,
    pub session_id: String,
    pub cwd: String,
    pub timestamp: Option<String>,
    pub output_bytes: Option<usize>,
}

pub struct DiscoverOpts {
    pub limit: usize,
    pub since_days: Option<u32>,
    pub all_projects: bool,
    pub current_dir: Option<PathBuf>,
    pub min_count: u64,
}
```

### Module: `types::obfsck`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditHit {
    pub label: String,
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct FilterSuggestion {
    pub pattern: String,
    pub label: String,
}
```

### Module: `types::rtk`

```rust
pub struct RtkDiscoverReport { /* 4 fields */ }
pub struct RtkSupportedEntry { /* 6 fields */ }
pub struct RtkUnsupportedEntry { /* 3 fields */ }
pub struct RtkGainReport { /* 4 fields */ }
pub struct RtkGainEntry { /* 4 fields */ }
pub struct RtkSessionEntry { /* 5 fields */ }
pub struct RtkVerifyResult { /* 3 fields */ }
pub struct RtkHookAudit { /* 1 field */ }
pub struct RtkAuditEntry { /* 3 fields */ }
pub struct RtkProbeResult { /* 4 fields */ }
```

### Module: `types::pipeline`

```rust
pub enum HookEvent {
    PreToolUse, PostToolUse, SessionStart, SessionEnd,
    PreCompact, Stop, SubagentStop,
}

pub enum When { Pre, Post }

pub enum HookAction { /* variants from pipeline.rs */ }

pub struct HookRule { /* fields from pipeline.rs */ }

pub struct HookPipelineConfig {
    pub hooks: Vec<HookRule>,
}

pub struct HookContext {
    pub event: Option<HookEvent>,
    pub tool_name: Option<String>,
    pub target: Option<String>,
    pub exit_code: Option<i64>,
    pub raw_json: Option<String>,
}

pub struct PipelineResult {
    pub deny: Option<String>,
    pub rewrite: Option<String>,
    pub messages: Vec<String>,
    pub matched_rules: Vec<String>,
}

pub struct HookDiagnostic {
    pub rule_index: usize,
    pub label: String,
    pub message: String,
    pub level: DiagLevel,
}

pub enum DiagLevel { Error, Warning }
```

### Port Traits (all with associated error types)

```rust
// types::ports::rules
pub trait RulesLoader {
    type Error: std::fmt::Debug;
    fn load(&self) -> Result<RulesConfig, Self::Error>;
}

// types::ports::state
pub trait StateStore {
    type Error: std::fmt::Debug;
    fn load(&self) -> Result<State, Self::Error>;
    fn save(&self, state: &State) -> Result<(), Self::Error>;
}

// types::ports::capture
pub trait CaptureStore {
    type Error: std::fmt::Debug;
    fn record(&self, record: SuggestionRecord)
        -> Result<(), Self::Error>;
    fn mark_accepted(
        &self, session_id: &str, command: &str, exit_code: i64,
    ) -> Result<(), Self::Error>;
}

// types::ports::history
pub trait CommandSource {
    fn commands(&self) -> impl Iterator<Item = CommandRecord>;
}

// types::ports::stats
pub trait StatsStore {
    type Error: std::fmt::Debug;
    fn load(&self) -> Result<Stats, Self::Error>;
    fn save(&self, stats: &Stats) -> Result<(), Self::Error>;
}

// types::ports::filters
pub trait FiltersLoader {
    type Error: std::fmt::Debug;
    fn load(&self) -> Result<FiltersConfig, Self::Error>;
    fn filters_path(&self) -> Option<PathBuf>;
}

// types::ports::obfsck
pub trait ObfsckMcp {
    fn audit(&self, text: &str) -> Vec<AuditHit>;
    fn generate_filters(&self, examples: &[String])
        -> Vec<FilterSuggestion>;
}

// types::ports::rtk
pub trait RtkAnalysis {
    fn discover(&self, since_days: u32)
        -> Option<RtkDiscoverReport>;
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

// types::ports::expand
pub trait VarExpander {
    fn expand(&self, command: &str) -> String;
}

// types::ports::tool_swap
pub trait FileInfo {
    fn file_size(&self, path: &str) -> Option<u64>;
    fn count_lines(&self, path: &str) -> Option<usize>;
    fn avg_bytes_per_line(&self, path: &str) -> Option<usize>;
}
```

## Data Flow

1. **Hook input**: stdin JSON -> `HookPayload` (deserialized in binary)
2. **Rule check**: `HookPayload` -> `RulesLoader::load()` -> `check_pipeline()` (in `coursers-core`)
3. **Hook output**: `protocol::deny_response()` / `protocol::rewrite_response()` -> stdout JSON
4. **State**: `StateStore::load()` -> `check_learned()` / `record_failure()` -> `StateStore::save()`

## Hexagonal Boundaries

### Ports (traits in `coursers-types`)

`RulesLoader`, `StateStore`, `CaptureStore`, `CommandSource`,
`StatsStore`, `FiltersLoader`, `ObfsckMcp`, `RtkAnalysis`,
`RtkRewrite`, `VarExpander`, `FileInfo`

### Adapters (impls in `coursers-core`)

`FsRulesLoader`, `ProfileFsRulesLoader`, `FsStateStore`,
`SuggestionStore`, `FsStatsStore`, `FsFiltersLoader`,
`JsonlCommandSource`, `EnvExpander`, `RealFileInfo`,
`NullObfsckMcpClient`, `NullRtkClient`

### Adapters (impls in `coursers` binary)

`obfsck::process::*` (concrete ObfsckMcp), `rtk::process::*`
(concrete RtkAnalysis/RtkRewrite), `nu_check::*`

### Test doubles (in `coursers-core`, `cfg(test)` or `feature = "testing"`)

`InMemoryRulesLoader`, `InMemoryStateStore`, `InMemoryCaptureStore`,
`InMemoryStatsStore`, `InMemoryFiltersLoader`, `FakeFileInfo`

## Binary Merge

The merged `coursers` binary exposes all subcommands:

```
coursers pre [--profile P] [--rules R] [--state S]
coursers post [--profile P] [--rules R] [--state S]
coursers filter [--profile P] [--rules R] [--state S]
coursers rewrite [--profile P] [--rules R]
coursers discover [--profile P] [--rules R] [-a] [-l N] [-s D] [-f F]
coursers validate [--profile P] [--rules R]
coursers probe [--profile P] [--rules R]
coursers stats [--profile P]
coursers insights [-f F] [-s D] [-r R]
coursers audit [--remove K]
coursers suggest [--profile P] [--rules R] [-a] [-s D] [-l N] [-f F]
coursers history [-l N] [-r R] [-f F]
coursers export [-o F]
coursers hook <event>
coursers validate-hooks [--target T]
coursers log [-l N] [-e E] [-o O] [-f F] [--prune-hours H]
coursers heat [-r R]
```

**Symlink**: `crs` -> `coursers`. Detection in `main()`:

```rust
fn main() {
    // argv[0] dispatch: if invoked as "crs", skip "pre"/"post"
    // subcommands (backwards compat for hook configs using
    // "crs filter", "crs rewrite" etc.)
    let cli = Cli::parse();
    // ... dispatch
}
```

No argv[0] detection needed -- clap handles unknown subcommands
gracefully. The symlink just makes `crs filter` resolve to
`coursers filter` at the OS level.

## Migration Sequence (7 commits, each green)

1. Create `coursers-types` with types + traits. Re-export from old
   paths in `crs-core` for compat.
2. Rename `crs-core` to `coursers-core`. Update all `use` paths.
3. Move `FilterResult`, `FilterPayload` to types. Move `run_filter`,
   `apply_filter` to core.
4. Move `jsonl_source` to core. Move `nu_check`, `obfsck`, `rtk`
   adapters to binary. Delete `crates/crs/src/lib.rs`.
5. Merge `crs` subcommands into `coursers` binary.
6. Add `crs` symlink (cargo `[[bin]]` alias). Rename `e2e_placeholder`
   to `coursers-e2e`.
7. Remove `crates/crs/`. Update workspace members.

## Out of Scope

- Testing gaps (property, integration, conformance) -- follow-up
- Hook config migration tooling for end users
- Profile/protocol changes beyond what's already landed
- `StatsStore::record_block` default method -- stays as-is for now

## Risk

- [x] Breaking API changes: yes -- all `use crs_core::` paths change;
      all port trait signatures gain `type Error`. Mitigated by re-exports
      in step 1 and mechanical rename in step 2.
- [ ] New external dependency: no
- [ ] Feature flag required: no
- [x] Semver: this is a 0.x workspace, no external consumers
