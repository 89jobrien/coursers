# Design: Canonical Contracts and Published Hook Adapters

> Pipeline contract details for `cca-03` are superseded by
> `docs/designs/2026-09-12-pipeline-contracts-design.md`.

## Goal

Make `coursers-types` the canonical contract crate and publish a focused
`coursers-adapters` crate for hook-path I/O without leaking infrastructure dependencies into
domain logic. Harness response shapes remain stable; approved fail-closed cases intentionally
change behavior when configured policy or redaction cannot be enforced.

## Approved Approach

Use types-first vertical slices, publish hook-path adapters immediately, preserve deprecated
`coursers-core` compatibility re-exports for one minor release, and defer non-hook I/O to the
localized TODOs already recorded in the workspace.

## Contents

- [Context Map](#context-map)
- [Crate Ownership](#crate-ownership)
- [Public API](#public-api)
- [Secure Failure Semantics](#secure-failure-semantics)
- [Data Flow](#data-flow)
- [Hexagonal Boundaries](#hexagonal-boundaries)
- [Migration Slices](#migration-slices)
- [Out of Scope](#out-of-scope)
- [Risk](#risk)

## Context Map

### Files to Modify

| File | Purpose | Changes Needed |
| --- | --- | --- |
| `Cargo.toml` | Workspace membership and shared dependencies | Add `crates/adapters` and its workspace dependency |
| `crates/types/src/rules.rs` | Rule and failure-learning contracts | Add `task_override`; become canonical |
| `crates/types/src/filters.rs` | Filter, rewrite, and tool-swap contracts | Become canonical for hook configuration |
| `crates/types/src/pipeline.rs` | Generic hook-pipeline contracts | Add all runtime actions and source-origin metadata |
| `crates/types/src/ports.rs` | Stable port traits | Add filter, rewrite, pipeline, process, and environment ports |
| `crates/types/src/state.rs` | Failure-learning persistence shape | Become the canonical state contract |
| `crates/types/src/config.rs` | Shared profile data | Remove divergent assumptions and retain protocol-neutral data only |
| `crates/types/src/lib.rs` | Public contract surface | Export the canonical modules and migration notes |
| `crates/adapters/Cargo.toml` | Published adapter package | Define metadata and dependencies |
| `crates/adapters/src/lib.rs` | Adapter public surface | Export hook-path adapters and `AdapterError` |
| `crates/adapters/src/config.rs` | Path and source resolution | Resolve explicit, project, and global sources |
| `crates/adapters/src/fs.rs` | Filesystem adapters | Implement rules, state, filters, rewrites, and pipeline loaders |
| `crates/adapters/src/process.rs` | Bounded subprocess adapter | Enforce timeout and output limits without shell evaluation |
| `crates/adapters/src/environment.rs` | Runtime context adapter | Read cwd, selected environment values, and Git branch context |
| `crates/core/src/rules.rs` | Rule evaluation | Import canonical contracts and retain deprecated re-exports |
| `crates/core/src/state.rs` | Failure-learning state transitions | Operate on canonical state contracts |
| `crates/core/src/loader.rs` | Current rules adapters | Remove concrete filesystem implementation after compatibility window |
| `crates/core/src/store.rs` | Current state adapters | Remove concrete filesystem implementation after compatibility window |
| `crates/core/src/hook/filters.rs` | Filter domain plus filesystem I/O | Keep filtering logic; move loaders to adapters |
| `crates/core/src/hook/rewrite.rs` | Rewrite domain plus filesystem I/O | Keep rewriting logic; move loaders to adapters |
| `crates/core/src/hook/pipeline.rs` | Pipeline evaluation plus I/O | Inject config, environment, and process ports |
| `crates/core/src/hook/chain.rs` | Hook execution and diagnostics | Return structured diagnostics to the application boundary |
| `crates/core/src/hook/concrete.rs` | Hook-chain composition units | Use canonical port traits and typed failures |
| `crates/core/src/hook/tool_swap.rs` | Tool-swap behavior | Consume canonical `ToolSwapConfig` |
| `crates/core/src/hook/protocol.rs` | Current protocol translation | Retain compatibility wrappers while translation moves to CLI |
| `crates/core/src/config.rs` | Current path resolution/composition | Move resolution to adapters and composition to CLI |
| `crates/core/src/lib.rs` | Compatibility facade | Add deprecated re-exports for one minor release |
| `crates/coursers/Cargo.toml` | Composition-root dependencies | Depend on `coursers-adapters` |
| `crates/coursers/src/lib.rs` | CLI dispatch | Build runtime dependencies at the application boundary |
| `crates/coursers/src/crs_commands.rs` | Generic hook and validation commands | Use injected pipeline loaders and executors |
| `crates/coursers/src/hook/mod.rs` | Legacy hook store construction | Construct adapters explicitly |
| `crates/coursers/src/hook/chain_runner.rs` | Chain execution entry point | Accept an injected runtime instead of building I/O in core |
| `crates/coursers/src/opencode.rs` | OpenCode composition path | Use the same injected hook runtime |
| `schemas/crs-hooks.schema.json` | Hook configuration schema | Add runtime action parity and corrected semantics |

### Dependencies

The target dependency graph is:

```text
coursers-types <- coursers-core
coursers-types <- coursers-adapters
coursers-core  <- coursers
coursers-adapters <- coursers
```

`coursers-core` and `coursers-adapters` must not depend on each other. The `coursers` package is
the composition root and the only package that joins domain services to concrete hook adapters.

### Test Coverage

| Test area | Existing coverage | Required addition |
| --- | --- | --- |
| Rules loading | In-memory port conformance plus core loader unit tests | Add adapter tests for valid, optional-missing, required-missing, and malformed sources |
| State persistence | `crates/core/tests/conformance_state_store.rs` | Run the same suite against `FsStateStore` from adapters |
| Filters and rewrites | Inline tests plus binary integration tests | Add adapter conformance for source precedence and malformed files |
| Generic pipeline | `crates/core/src/hook/pipeline.rs` tests | Add fake process/environment ports and secure failure cases |
| Chain behavior | `crates/e2e/tests/hook_chain.rs` | Prove equivalent outcomes only for scenarios the current chain supports; keep documented legacy gaps explicit |
| Harness behavior | Codex and OpenCode integration tests | Prove adapter migration does not alter protocol output |

### Reference Patterns

- Associated-error port traits already exist in `crates/types/src/ports.rs`.
- Generic hook units already inject loaders and stores in
  `crates/core/src/hook/concrete.rs`.
- `ProfileConfig` already captures path precedence in `crates/core/src/config.rs`.
- `HookPipelineConfig::load_from` demonstrates current TOML parsing in
  `crates/core/src/hook/pipeline.rs`.

## Crate Ownership

- **`coursers-types`** owns serialized domain records, hook input/output records, source-origin
  metadata, process request/response records, and stable port traits.
- **`coursers-core`** owns deterministic rule matching, rewriting, filtering, failure-learning
  transitions, pipeline evaluation, and security policy decisions.
- **`coursers-adapters`** owns concrete filesystem, environment, Git-context, and bounded-process
  implementations for the runtime hook path. It is published immediately.
- **`coursers`** owns Clap, stdin/stdout protocol translation, harness-specific responses, and the
  composition root.

Non-hook analysis persistence, redb logging, JSONL sessions, prefix stores, RTK, and obfsck MCP
clients are explicitly out of the first adapter release.

## Public API

### Canonical Contract Updates

The existing `Rule`, `FailureLearning`, `RulesConfig`, `State`, filter, rewrite, and pipeline types
in `coursers-types` become canonical. Their current core counterparts become deprecated re-exports.
`Rule` gains the existing core field:

```rust
pub struct Rule {
    pub id: String,
    pub enabled: bool,
    pub pattern: String,
    pub pattern_flags: String,
    pub exceptions: Vec<String>,
    pub target_commands: Vec<String>,
    pub message: Option<String>,
    pub task_override: Option<String>,
}
```

`HookAction` includes every runtime variant before it becomes canonical:

```rust
pub enum HookAction {
    Deny { message: String },
    Rewrite {
        inject: Option<String>,
        prepend: Option<String>,
        replace: Option<String>,
    },
    Run { command: Vec<String>, capture: bool },
    Notify { template: String },
    Redact { level: Option<String> },
}
```

### Source and Security Types

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConfigOrigin {
    Explicit,
    Project,
    Global,
    GlobalPlugin,
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SourceRequirement {
    Optional,
    Required,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FilterSourceMode {
    SelectFirstAvailable,
    MergeAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProjectActionAuthorization {
    Deny,
    Allow,
}

#[derive(Debug, Clone)]
pub struct LoadedHookRule {
    rule: HookRule,
    origin: ConfigOrigin,
}

impl LoadedHookRule {
    pub fn new(rule: HookRule, origin: ConfigOrigin) -> Self;
    pub fn rule(&self) -> &HookRule;
    pub fn origin(&self) -> ConfigOrigin;
}

#[derive(Debug, Clone, Default)]
pub struct LoadedHookPipeline {
    hooks: Vec<LoadedHookRule>,
}

impl LoadedHookPipeline {
    pub fn new(hooks: Vec<LoadedHookRule>) -> Self;
    pub fn hooks(&self) -> &[LoadedHookRule];
}
```

Rule provenance survives merging through `LoadedHookRule`, allowing core to authorize each action
using its actual source. Project-local rewrite and executable actions receive
`ProjectActionAuthorization::Deny` by default. This design does not add a trust-management command;
a later feature may supply an explicit authorization.

### Pipeline Parity

The canonical pipeline context and result include the output fields already required by runtime
redaction:

```rust
#[derive(Debug, Clone, Default)]
pub struct HookContext {
    pub event: Option<HookEvent>,
    pub tool_name: Option<String>,
    pub target: Option<String>,
    pub exit_code: Option<i64>,
    pub raw_json: Option<String>,
    pub output: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PipelineResult {
    pub deny: Option<String>,
    pub rewrite: Option<String>,
    pub replace_output: Option<String>,
    pub messages: Vec<String>,
    pub matched_rules: Vec<String>,
    pub notices: Vec<HookRuntimeNotice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HookRuntimeNoticeLevel {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRuntimeNotice {
    level: HookRuntimeNoticeLevel,
    component: String,
    message: String,
}

impl HookRuntimeNotice {
    pub fn new(
        level: HookRuntimeNoticeLevel,
        component: impl Into<String>,
        message: impl Into<String>,
    ) -> Self;
    pub fn level(&self) -> HookRuntimeNoticeLevel;
    pub fn component(&self) -> &str;
    pub fn message(&self) -> &str;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookExecution<T> {
    outcome: T,
    notices: Vec<HookRuntimeNotice>,
}

impl<T> HookExecution<T> {
    pub fn new(outcome: T, notices: Vec<HookRuntimeNotice>) -> Self;
    pub fn outcome(&self) -> &T;
    pub fn notices(&self) -> &[HookRuntimeNotice];
    pub fn into_outcome(self) -> T;
}
```

### Hook Environment

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookEnvironment {
    cwd: PathBuf,
    git_branch: Option<String>,
    variables: BTreeMap<String, String>,
    project_action_authorization: ProjectActionAuthorization,
}

impl HookEnvironment {
    pub fn new(
        cwd: PathBuf,
        git_branch: Option<String>,
        variables: BTreeMap<String, String>,
        project_action_authorization: ProjectActionAuthorization,
    ) -> Self;
    pub fn cwd(&self) -> &Path;
    pub fn git_branch(&self) -> Option<&str>;
    pub fn variable(&self, name: &str) -> Option<&str>;
    pub fn project_action_authorization(&self) -> ProjectActionAuthorization;
}
```

### Process Port

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessLimits {
    timeout: Duration,
    stdout_bytes: usize,
    stderr_bytes: usize,
}

impl ProcessLimits {
    pub fn new(timeout: Duration, stdout_bytes: usize, stderr_bytes: usize) -> Self;
    pub fn timeout(&self) -> Duration;
    pub fn stdout_bytes(&self) -> usize;
    pub fn stderr_bytes(&self) -> usize;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRequest {
    program: OsString,
    args: Vec<OsString>,
    stdin: Vec<u8>,
}

impl ProcessRequest {
    pub fn new(program: impl Into<OsString>) -> Self;
    pub fn with_args<I, S>(self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>;
    pub fn with_stdin(self, stdin: impl Into<Vec<u8>>) -> Self;
    pub fn program(&self) -> &OsStr;
    pub fn args(&self) -> &[OsString];
    pub fn stdin(&self) -> &[u8];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutput {
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_truncated: bool,
    stderr_truncated: bool,
}

impl ProcessOutput {
    pub fn new(
        exit_code: Option<i32>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        stdout_truncated: bool,
        stderr_truncated: bool,
    ) -> Self;
    pub fn exit_code(&self) -> Option<i32>;
    pub fn stdout(&self) -> &[u8];
    pub fn stderr(&self) -> &[u8];
    pub fn stdout_truncated(&self) -> bool;
    pub fn stderr_truncated(&self) -> bool;
}

pub trait ProcessRunner {
    type Error: std::error::Error + Send + Sync + 'static;

    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, Self::Error>;
}
```

`ProcessRequest` takes an executable and argument vector, never a shell command string.
`BoundedProcessRunner` applies its constructor-supplied limits to every request, drains stdout and
stderr concurrently, terminates timed-out children, and records truncation instead of allocating
without bounds. Zero is valid for each limit: a zero timeout requests immediate termination, while
a zero byte limit captures nothing and marks any received bytes as truncated. A timeout returns
`AdapterErrorKind::ProcessTimeout` after successful child cleanup; spawn failures return
`ProcessSpawn`; stdin-write, pipe-read, kill, and post-kill wait failures return `ProcessIo`.

### Loader Ports

Existing `RulesLoader`, `StateStore`, and `FiltersLoader` retain their associated-error shape so
current external implementations remain valid. `FiltersLoader` also retains `filters_path`. The
canonical port module adds the other hook-path ports:

```rust
pub trait FiltersLoader {
    type Error: std::fmt::Debug;

    fn load(&self) -> Result<FiltersConfig, Self::Error>;
    fn filters_path(&self) -> Option<PathBuf>;
}

pub trait AtomicStateStore: StateStore {
    fn update(
        &self,
        update: &mut dyn FnMut(State) -> State,
    ) -> Result<State, Self::Error>;
}

pub trait RewriteLoader {
    type Error: std::error::Error + Send + Sync + 'static;

    fn load(&self) -> Result<RewriteConfig, Self::Error>;
}

pub trait HookPipelineLoader {
    type Error: std::error::Error + Send + Sync + 'static;

    fn load(&self) -> Result<LoadedHookPipeline, Self::Error>;
}

pub trait HookEnvironmentProvider {
    type Error: std::error::Error + Send + Sync + 'static;

    fn load(&self, variable_names: &BTreeSet<String>) -> Result<HookEnvironment, Self::Error>;
}
```

The pipeline evaluator extracts referenced `${VAR}` names from loaded templates and passes only
that set to `HookEnvironmentProvider`. `PWD` and `GIT_BRANCH` remain reserved fields. This preserves
arbitrary configured variable expansion without copying unrelated environment values into hook
context.

`AtomicStateStore` is additive. `FsStateStore` holds one exclusive lock across read, mutation, and
same-directory temporary-file replacement so concurrent hook processes cannot lose updates. The
existing `StateStore` remains sufficient for read-only consumers and external implementations.

### Adapter Types

`coursers-adapters` exposes private-field concrete adapters with constructors rather than public
configuration fields:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AdapterErrorKind {
    Io,
    Parse,
    ProcessSpawn,
    ProcessIo,
    ProcessTimeout,
}

#[derive(Debug, thiserror::Error)]
#[error("{kind:?}: {message}")]
pub struct AdapterError {
    kind: AdapterErrorKind,
    message: String,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl AdapterError {
    pub fn kind(&self) -> AdapterErrorKind;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSource {
    path: PathBuf,
    origin: ConfigOrigin,
    requirement: SourceRequirement,
}

impl ConfigSource {
    pub fn new(path: PathBuf, origin: ConfigOrigin, requirement: SourceRequirement) -> Self;
    pub fn path(&self) -> &Path;
    pub fn origin(&self) -> ConfigOrigin;
    pub fn requirement(&self) -> SourceRequirement;
}

#[derive(Debug, Clone)]
pub struct FsRulesLoader {
    source: ConfigSource,
}

#[derive(Debug, Clone)]
pub struct FsStateStore {
    path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct FsFiltersLoader {
    sources: Vec<ConfigSource>,
    mode: FilterSourceMode,
}

#[derive(Debug, Clone)]
pub struct FsRewriteLoader {
    sources: Vec<ConfigSource>,
}

#[derive(Debug, Clone)]
pub struct FsHookPipelineLoader {
    sources: Vec<ConfigSource>,
}

#[derive(Debug, Clone)]
pub struct SystemHookEnvironment<P> {
    cwd: PathBuf,
    project_action_authorization: ProjectActionAuthorization,
    process_runner: P,
}

#[derive(Debug, Clone)]
pub struct BoundedProcessRunner {
    default_limits: ProcessLimits,
}
```

Their public constructors are:

```rust
impl FsRulesLoader {
    pub fn new(source: ConfigSource) -> Self;
}

impl FsStateStore {
    pub fn new(path: PathBuf) -> Self;
}

impl FsFiltersLoader {
    pub fn new(sources: Vec<ConfigSource>, mode: FilterSourceMode) -> Self;
}

impl FsRewriteLoader {
    pub fn new(sources: Vec<ConfigSource>) -> Self;
}

impl FsHookPipelineLoader {
    pub fn new(sources: Vec<ConfigSource>) -> Self;
}

impl<P> SystemHookEnvironment<P>
where
    P: ProcessRunner,
{
    pub fn new(
        cwd: PathBuf,
        project_action_authorization: ProjectActionAuthorization,
        process_runner: P,
    ) -> Self;
}

impl BoundedProcessRunner {
    pub fn new(default_limits: ProcessLimits) -> Self;
}
```

Path resolution replaces the current core-owned `ConfigBuilder` and `ProfileConfig`:

```rust
pub struct ResolvedHookSources {
    rules: ConfigSource,
    state_path: PathBuf,
    filters: Vec<ConfigSource>,
    rewrites: Vec<ConfigSource>,
    pipeline: Vec<ConfigSource>,
}

impl ResolvedHookSources {
    pub fn rules(&self) -> &ConfigSource;
    pub fn state_path(&self) -> &Path;
    pub fn filters(&self) -> &[ConfigSource];
    pub fn rewrites(&self) -> &[ConfigSource];
    pub fn pipeline(&self) -> &[ConfigSource];
}

pub struct HookSourcesBuilder {
    home: PathBuf,
    cwd: PathBuf,
    profile: Option<String>,
    rules_override: Option<PathBuf>,
    state_override: Option<PathBuf>,
}

impl HookSourcesBuilder {
    pub fn new(home: PathBuf, cwd: PathBuf) -> Self;
    pub fn profile(self, profile: impl Into<String>) -> Self;
    pub fn rules(self, path: PathBuf) -> Self;
    pub fn state(self, path: PathBuf) -> Self;
    pub fn build(self) -> Result<ResolvedHookSources, AdapterError>;
}
```

Source semantics remain subsystem-specific and are covered by conformance tests:

- Rules use one source: explicit override, named profile, or optional global default.
- State uses an explicit override, then an existing project-local file, then profile/global state.
- Filters use an environment override exclusively. Hook-chain/profile callers use
  `SelectFirstAvailable` with project before global, while the standalone legacy filter command
  uses `MergeAll` to preserve its current project-then-global behavior.
- Rewrites use an environment override exclusively; otherwise project-local and global content are
  merged in that order.
- Pipelines load global config, lexically sorted global plugins, then project config while retaining
  each rule's origin.

Harness protocol selection does not move into adapters. A `pub(crate)` resolver in `coursers`
preserves current behavior: an explicit hook target wins, the `codex` profile infers Codex when no
target is supplied, and all other profiles default to Claude. OpenCode continues to use its
target-specific response adapter.

No raw `dirs`, `redb`, parser-error, or child-process implementation types appear in public
signatures.

### Core Execution API

Core adds injected APIs alongside the existing wrappers:

```rust
pub struct PipelineExecutor<P, E> {
    process_runner: P,
    environment_provider: E,
}

impl<P, E> PipelineExecutor<P, E>
where
    P: ProcessRunner,
    E: HookEnvironmentProvider,
{
    pub fn new(process_runner: P, environment_provider: E) -> Self;
    pub fn execute(
        &self,
        pipeline: &LoadedHookPipeline,
        context: &HookContext,
    ) -> PipelineResult;
}

pub fn build_hook_chain<R, S, F, W>(
    rules_loader: R,
    state_store: S,
    filters_loader: F,
    rewrite_loader: W,
    running_titles: Vec<String>,
) -> HookChain
where
    R: RulesLoader + Clone + 'static,
    R::Error: 'static,
    S: AtomicStateStore + Clone + 'static,
    S::Error: 'static,
    F: FiltersLoader + 'static,
    F::Error: 'static,
    W: RewriteLoader + 'static,
    W::Error: 'static;

impl HookChain {
    pub fn run_pre_report(
        &self,
        context: &HookContext,
    ) -> Result<HookExecution<PreHookOutcome>, CourserError>;
    pub fn run_post_report(
        &self,
        context: &HookContext,
        output: &ToolOutput,
    ) -> Result<HookExecution<PostHookOutcome>, CourserError>;
}
```

The builder clones rules and state handles because both rule evaluation and the failure observer
use them; published filesystem handles therefore implement `Clone`. Port errors are converted
inside core to `CourserError::Adapter { port: &'static str, detail: String }`.

`PipelineExecutor::execute` extracts `${VAR}` references from the loaded pipeline, requests only
those names from its environment provider, and then evaluates templates and actions. The report
methods consume enforcement errors rather than returning them as passthroughs:

- `run_pre_report` converts rules, state-load, rewrite, and required-config adapter failures into
  `Ok(HookExecution::new(PreHookOutcome::Deny(...), notices))`.
- `run_post_report` converts filter or redaction-policy adapter failures into a filter outcome that
  withholds original output.
- `Err` is reserved for malformed application input or core invariant failures; CLI callers emit a
  protocol-safe denial or withholding response for those errors.

Observer failures become `HookRuntimeNotice` values so the CLI can write diagnostics without core
printing them. Existing `run_pre`, `run_post`, `run_pipeline`, `ConfigBuilder`, and `ProfileConfig`
wrappers retain their current behavior for the one-minor compatibility window.

The legacy core loader/store traits also remain as real deprecated traits for that release; they
are not aliases. Core-owned, `pub(crate)` bridge wrappers adapt those traits to the canonical ports
inside compatibility entry points. This keeps existing external implementations source-compatible
until the next minor release without adding another public facade.

## Secure Failure Semantics

Adapters report facts; core chooses policy:

1. An absent optional source returns the relevant empty configuration.
2. An explicit source, or an existing discovered source, that cannot be read or parsed returns an
   error. Pre-hook evaluation converts it into a deny outcome; post-hook evaluation withholds the
   original output when the failed policy could have filtered or redacted it.
3. A redaction process failure replaces output with a protocol-safe withholding diagnostic. The
   original output is never returned.
4. A project-origin `Rewrite` or `Run` action is denied unless project-action authorization is
   explicitly `Allow`.
5. A missing state file represents fresh state and loads as `State::default()`. An existing state
   file that is unreadable or malformed denies pre-hook evaluation because learned blocks cannot
   be checked. A state-update failure after execution is reported but cannot retroactively deny the
   completed tool call.
6. A trusted or global `Run` action that cannot spawn, times out, or exits unsuccessfully denies an
   event that supports blocking. For `PostToolUse`, the original output is withheld. Lifecycle
   events that cannot be reversed return an error notice for the harness adapter to report.
7. Logging, capture, stats, and notification failures remain non-blocking and write diagnostics to
   stderr at the application boundary.
8. Adapter and core code never print and never exit the process.

## Data Flow

1. The `coursers` CLI parses the harness payload and resolves profile arguments.
2. `coursers-adapters` resolves configuration sources and constructs filesystem, environment, and
   process adapters.
3. `coursers` injects those adapters into `coursers-core` hook services.
4. `coursers-core` loads canonical `coursers-types` records, evaluates policy, and returns a
   harness-neutral outcome.
5. `coursers` serializes that outcome using Claude, Codex, or OpenCode protocol rules.

## Hexagonal Boundaries

- **Ports**: loader, store, environment, and process traits in `coursers-types::ports`.
- **Domain services**: matching, filtering, rewriting, state transitions, and pipeline evaluation
  in `coursers-core`.
- **Adapters**: filesystem, environment, Git, and child-process implementations in
  `coursers-adapters`.
- **Composition root**: `coursers::run` and target-specific CLI modules.

## Migration Slices

1. **Rules and state contracts**: add missing fields to `coursers-types`; update core evaluation to
   use and re-export the canonical records. No adapter crate usage yet.
2. **Filter, tool-swap, and pipeline contracts**: remove duplicate hook configuration types, add
   source provenance and ports, and update the JSON Schema.
3. **Adapter package**: add and publish `coursers-adapters` with filesystem, environment, and
   process implementations plus temporary-resource conformance tests.
4. **Injected core APIs**: add `PipelineExecutor`, `build_hook_chain`, and report-returning chain
   methods alongside the existing I/O-building wrappers. Add secure failure tests without removing
   any current caller entry point.
5. **CLI composition**: construct adapters in `coursers`; migrate generic hooks, legacy pre/post,
   chain, Codex, and OpenCode callers; run all integration suites.
6. **Protocol ownership**: move harness response construction into `coursers` while retaining
   deprecated wrappers in `coursers-core`.
7. **Compatibility release**: document and deprecate core adapter, config-builder, protocol, and
   contract re-exports. Slice 3 is the first published `coursers-adapters` release.
8. **Next minor release**: remove the deprecated core re-exports and legacy composition wrappers.

Each implementation slice must remain buildable and should touch at most three crates. No slice may
leave duplicate independently deserializable contract definitions.

## Out of Scope

- Analysis capture and stats persistence.
- redb hook-log storage.
- JSONL session discovery and reading.
- Prefix-learning persistence.
- RTK and obfsck MCP clients outside hook redaction.
- A repository trust-management command or persistent trust database.
- Unifying the legacy and opt-in hook-chain execution paths.
- Changing Claude, Codex, or OpenCode response schemas.

## Risk

- **Breaking API changes**: deferred for one minor release through deprecated core re-exports.
- **Serialization changes**: additive `task_override` and runtime action parity; require round-trip
  and backward-compatibility tests.
- **New published crate**: yes, `coursers-adapters`; it must include crate-level docs, examples,
  metadata, and release notes from its first release.
- **New external dependency**: no requirement in the public design; bounded execution can use
  standard-library process and thread primitives.
- **Feature flags**: none for the initial hook-only crate; add features only when later adapters
  introduce substantial optional dependencies.
- **Behavioral change**: malformed configured security policy and failed redaction become
  fail-closed by design. Project-local `Rewrite` and `Run` actions that currently execute are also
  denied until an explicit trust feature supplies authorization.
- **Cross-crate scope**: the final architecture spans four crates, but every migration slice is
  constrained to three crates or fewer and retains buildable compatibility wrappers.
- **Public API quality**: the adapter crate requires crate-level documentation, examples using
  temporary directories, `# Errors` sections, common trait implementations, and a documented
  `#[non_exhaustive]` policy before its first publication.
