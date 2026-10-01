# Design: Canonical Pipeline Contracts

## Goal

Make the generic hook-pipeline contracts canonical in `coursers-types`, with complete action
parity, output-only source provenance, serializable execution reports, and schema-backed tests,
without changing runtime behavior in `cca-03`.

## Approved Approach

Use the **Immediate Module Split** approach: separate configuration, provenance, execution, and
validation contracts under `coursers_types::pipeline`, while preserving current import paths
through re-exports.

## Contents

- [Context Map](#context-map)
- [Crate Ownership](#crate-ownership)
- [Public API](#public-api)
- [Schema Contract](#schema-contract)
- [Data Flow](#data-flow)
- [Hexagonal Boundaries](#hexagonal-boundaries)
- [Test Design](#test-design)
- [Integration Points](#integration-points)
- [Out Of Scope](#out-of-scope)
- [Risk](#risk)

## Context Map

### Files Affected

| File | Purpose | Operation |
| --- | --- | --- |
| `crates/types/src/pipeline.rs` | Current flat pipeline contracts | Replace with the `pipeline/` module directory |
| `crates/types/src/pipeline/mod.rs` | Compatibility surface | Create; re-export all pipeline contract names |
| `crates/types/src/pipeline/config.rs` | Serialized policy configuration | Create; own events, timing, actions, rules, and root config |
| `crates/types/src/pipeline/provenance.rs` | Trusted loaded-rule metadata | Create; own origins and loaded wrappers |
| `crates/types/src/pipeline/execution.rs` | Runtime inputs and reports | Create; own context, outcomes, notices, and safe summaries |
| `crates/types/src/pipeline/validation.rs` | Config validation records | Create; own validation diagnostic types |
| `crates/types/src/lib.rs` | Types crate entry point | Continue exporting `pipeline` from its existing path |
| `crates/types/Cargo.toml` | Types test dependencies | Add `static_assertions` as a development-only dependency |
| `crates/types/tests/pipeline_contract.rs` | Contract fixtures and compile assertions | Create; test exact shapes and non-deserializable provenance |
| `crates/types/tests/fixtures/pipeline-actions.toml` | Canonical config fixture | Create; cover all actions and optional fields |
| `crates/types/tests/fixtures/pipeline-actions.json` | Exact serialized config fixture | Create; lock the emitted JSON shape |
| `crates/types/tests/fixtures/legacy-pipeline.toml` | Backward-compatibility fixture | Create; omit additive fields and provenance |
| `crates/types/tests/fixtures/execution-summary.json` | Sanitized report fixture | Create; lock the safe report shape |
| `crates/core/src/hook/pipeline.rs` | Loading, evaluation, validation, and side effects | Remove duplicate records and re-export canonical contracts |
| `crates/core/tests/property_tests.rs` | Existing proptest suite | Add config round-trip and merge-order properties |
| `crates/core/tests/pipeline_compat.rs` | Core facade compatibility | Create; compile old imports, literals, methods, and signatures |
| `crates/core/tests/hook_schema_contract.rs` | Schema drift coverage | Create; verify runtime actions and documented semantics |
| `schemas/crs-hooks.schema.json` | Editor/tooling contract | Add redaction and correct regex/template descriptions |
| `docs/designs/2026-09-12-canonical-contracts-hook-adapters-design.md` | Parent architecture design | Mark this pipeline-specific design as authoritative for `cca-03` |

### Dependencies And Consumers

- `crates/core/src/lib.rs` re-exports `hook::pipeline` as `coursers_core::hook_pipeline`.
- `crates/coursers/src/crs_commands.rs` uses the core compatibility path for loading, validation,
  event parsing, context construction, and result handling.
- `crates/coursers/src/opencode.rs` constructs `HookContext`, runs the pipeline, and consumes all
  current result fields including `replace_output`.
- Core pipeline unit tests cover loading, merging, matching, validation, and representative action
  behavior; `cca-03` adds explicit serialized fixtures for every action and the redaction contract.
- `crates/e2e/tests/pipeline.rs` covers binary-level behavior but does not consume the Rust contract
  types directly.
- No production code currently consumes provenance or runtime-notice wrappers.

### Existing Reference Patterns

- `crates/types/src/filters.rs` uses canonical records plus a temporary `core-compat` loading
  helper and core re-exports.
- `crates/core/src/hook/filter_logic.rs` demonstrates importing canonical types while retaining
  core-owned behavior.
- `crates/core/tests/property_tests.rs` is the existing location for cross-domain proptest coverage.

### Risk Flags

- Adding `Redact` to the public `HookAction` enum is source-breaking for exhaustive downstream
  matches. Adding `output` and `replace_output` also breaks complete external struct literals and
  exhaustive patterns, although the workspace is pre-1.0.
- Adding `Serialize` establishes a stable emitted representation for configuration and sanitized
  execution-summary records.
- Moving from a flat file to a module directory must preserve `coursers_types::pipeline::*` and
  `coursers_core::hook_pipeline::*` paths.
- Provenance must never be deserialized from untrusted configuration.
- `HookPipelineConfig::load_from` temporarily retains filesystem I/O under `core-compat`; the
  adapters migration removes it later.

## Crate Ownership

- **Owner**: `coursers-types` owns all plain pipeline configuration, provenance, execution, and
  validation records.
- **Behavior owner**: `coursers-core` retains source discovery, TOML loading orchestration, rule
  evaluation, validation algorithms, environment access, redaction, and subprocess execution.
- **Unaffected composition**: `coursers` continues consuming the same core compatibility paths.

This design affects two Rust crates. It adds one development-only dependency,
`static_assertions = "1"`, and adds zero production dependencies or I/O ports.

## Public API

### Configuration Module

`coursers_types::pipeline::config` defines:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    SessionStart,
    SessionEnd,
    PermissionRequest,
    PreCompact,
    PostCompact,
    UserPromptSubmit,
    SubagentStart,
    Stop,
    SubagentStop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum When {
    #[default]
    Always,
    OnSuccess,
    OnFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum HookAction {
    Deny {
        message: String,
    },
    Rewrite {
        #[serde(default)]
        inject: Option<String>,
        #[serde(default)]
        prepend: Option<String>,
        #[serde(default)]
        replace: Option<String>,
    },
    Run {
        command: Vec<String>,
        #[serde(default)]
        capture: bool,
    },
    Notify {
        template: String,
    },
    Redact {
        #[serde(default)]
        level: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookRule {
    pub event: HookEvent,
    #[serde(default)]
    pub matcher: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub unless: Option<String>,
    #[serde(default)]
    pub when: When,
    #[serde(flatten)]
    pub action: HookAction,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookPipelineConfig {
    #[serde(default)]
    pub hooks: Vec<HookRule>,
}

impl HookPipelineConfig {
    pub fn merge(&mut self, other: Self);

    #[cfg(feature = "core-compat")]
    pub fn load_from(path: &Path) -> Self;
}
```

Serialization uses the field names and tagged action representation already established by current
deserialization. `load_from` preserves missing-or-malformed-to-default behavior until
`coursers-adapters` supplies its replacement.

### Provenance Module

`coursers_types::pipeline::provenance` defines output-only runtime metadata:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ConfigOrigin {
    Explicit,
    Project,
    Global,
    GlobalPlugin,
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoadedHookRule {
    rule: HookRule,
    origin: ConfigOrigin,
}

impl LoadedHookRule {
    pub fn new(rule: HookRule, origin: ConfigOrigin) -> Self;
    pub fn rule(&self) -> &HookRule;
    pub fn origin(&self) -> ConfigOrigin;
    pub fn into_rule(self) -> HookRule;
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LoadedHookPipeline {
    hooks: Vec<LoadedHookRule>,
}

impl LoadedHookPipeline {
    pub fn new(hooks: Vec<LoadedHookRule>) -> Self;
    pub fn hooks(&self) -> &[LoadedHookRule];
    pub fn into_hooks(self) -> Vec<LoadedHookRule>;
}
```

These wrappers intentionally do not implement `Deserialize`. Trusted loaders may construct them in
Rust, but TOML and JSON policy input cannot create a loaded wrapper. A plain `HookRule` continues
ignoring unknown fields for backward compatibility; an input `origin` field is discarded and never
becomes trusted provenance.

### Execution Module

`coursers_types::pipeline::execution` keeps secret-bearing runtime values non-serializable and
provides a separate sanitized summary for logs and exports:

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    pub messages: Vec<String>,
    pub matched_rules: Vec<String>,
    pub replace_output: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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
    pub fn into_parts(self) -> (T, Vec<HookRuntimeNotice>);
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookExecutionSummary {
    denied: bool,
    rewritten: bool,
    output_replaced: bool,
    message_count: usize,
    matched_rule_count: usize,
    warning_count: usize,
    error_count: usize,
}

impl HookExecutionSummary {
    pub fn from_pipeline(execution: &HookExecution<PipelineResult>) -> Self;
    pub fn denied(&self) -> bool;
    pub fn rewritten(&self) -> bool;
    pub fn output_replaced(&self) -> bool;
    pub fn message_count(&self) -> usize;
    pub fn matched_rule_count(&self) -> usize;
    pub fn warning_count(&self) -> usize;
    pub fn error_count(&self) -> usize;
}
```

`HookContext`, `PipelineResult`, `HookRuntimeNotice`, and `HookExecution<T>` intentionally do not
implement Serde because they may contain raw payloads, command rewrites, tool output, or detailed
errors. `HookExecutionSummary` records only booleans and counts; it never includes command text,
output text, rule labels, component names, denial messages, notification text, or diagnostic
details. Consequently, the `matched_rules` and `component` strings shown in the non-Serde runtime
types cannot enter the serialized summary.

### Validation Module

`coursers_types::pipeline::validation` defines:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookDiagnostic {
    pub level: DiagLevel,
    pub rule_index: usize,
    pub label: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagLevel {
    Error,
    Warning,
}
```

Validation diagnostics remain distinct from runtime notices because they identify a configuration
rule index and label rather than an executing component.

### Compatibility Re-exports

`coursers_types::pipeline::mod.rs` publicly re-exports every type above. Core publicly re-exports
the canonical types from `crates/core/src/hook/pipeline.rs`, preserving these existing paths:

```text
coursers_types::pipeline::HookAction
coursers_types::pipeline::HookPipelineConfig
coursers_core::hook_pipeline::HookAction
coursers_core::hook_pipeline::HookPipelineConfig
```

No new public traits or free functions are introduced in `cca-03`.

## Schema Contract

`schemas/crs-hooks.schema.json` changes as follows:

1. Add `RedactAction` to `HookAction.oneOf`.
2. Define `action = "redact"` with optional string `level`.
3. Describe `unless` as a regular expression, not a substring.
4. Document notify placeholders as `${target}`, `${tool_name}`, and `${exit_code}`.
5. Document matcher syntax as `*`, an exact tool name, or exact `|`-separated alternatives; it is
   not a general glob.
6. Document `${VAR}` and `${GIT_BRANCH_SLUG}` expansion for rewrite injection/prepending.

The schema remains descriptive. Runtime validation continues to be authoritative for regex
compilation and action-specific semantic checks.

## Data Flow

1. Existing core loading reads TOML into canonical `HookPipelineConfig` values.
2. Existing core evaluation reads canonical rules and context, then returns canonical
   `PipelineResult`.
3. Existing CLI callers consume the same core compatibility paths and protocol behavior.
4. Future adapter loaders attach `ConfigOrigin` and produce `LoadedHookPipeline`.
5. Future executor and chain APIs wrap outcomes in `HookExecution<T>`.

## Hexagonal Boundaries

- **Contract boundary**: all plain data lives in `coursers-types`.
- **Domain behavior**: source-loading orchestration, matching, validation, and action evaluation
  remain in `coursers-core`; pure config concatenation lives on the canonical config type.
- **Temporary compatibility I/O**: only `HookPipelineConfig::load_from` is feature-gated in types;
  it is removed after the adapter migration and deprecation window.
- **No new adapter**: provenance attachment and runtime notice production are not wired here.

## Test Design

### Example-Based Tests

- `crates/types/tests/pipeline_contract.rs` parses `pipeline-actions.toml`, compares exact values for
  every action, and compares serialization with `pipeline-actions.json`.
- The same types test parses `legacy-pipeline.toml`, proving omitted additive fields retain current
  defaults.
- The types test serializes loaded rules and verifies the exact origin classification, then uses
  `static_assertions::assert_not_impl_any!` to prove loaded wrappers do not implement
  `DeserializeOwned`.
- The types test parses a plain rule containing an unknown `origin`, verifies the field is ignored,
  and verifies no loaded wrapper is produced.
- The types test compares an exact sanitized `HookExecutionSummary` with
  `execution-summary.json`; secret-bearing context, outcomes, notices, and generic execution
  wrappers have no Serde tests because they do not implement Serde.
- Core unit tests assert core and types paths have identical `TypeId` values and preserve loading,
  merge, validation, lint, matching, action, redaction-command, and no-output behavior. This task
  does not execute an installed `obfsck` binary or mutate `PATH`.
- `crates/core/tests/pipeline_compat.rs` imports every contract through
  `coursers_core::hook_pipeline`, constructs existing public struct literals, calls `load_from`, and
  assigns `load_config`, `run_pipeline`, `validate_config`, and `lint_config` to their current
  function-pointer signatures.
- `crates/core/tests/hook_schema_contract.rs` parses the schema and asserts the exact action `oneOf`
  references, required fields, property types, event values, `when` values, rewrite `anyOf`, redact
  level shape, regex wording, matcher wording, and placeholder syntax.
- Every moved public type, variant, field, and method retains or improves its current rustdoc; new
  runtime and provenance types document security and serialization constraints.

### Property Tests

Add proptest generators in `crates/core/tests/property_tests.rs` for `HookEvent`, `When`, all five
`HookAction` variants, optional rule fields, and bounded strings. Verify:

1. Any generated `HookPipelineConfig` round-trips through TOML without changing value.
2. Any generated `HookExecutionSummary` round-trips through JSON without changing value.
3. Merging generated config vectors preserves exact left-to-right rule order.
4. Merge is associative for rule ordering: `(a + b) + c` equals `a + (b + c)`.

Generated strings are limited to printable, non-control characters accepted by TOML basic strings.
The property suite uses the existing `proptest` development dependency in `coursers-core`.
`coursers-types` adds only the development dependency `static_assertions = "1"`; production
dependencies remain unchanged.

Source precedence is not claimed or tested in `cca-03`: current tests do not cover the complete
global -> sorted plugins -> project sequence. This task does not change `load_config`; provenance
attachment and source-order conformance belong to the adapter-loading task.

## Integration Points

- Core's current `load_config`, `run_pipeline`, `validate_config`, `lint_config`, and
  `config_source_paths` signatures remain unchanged.
- `crates/coursers/src/crs_commands.rs` and `crates/coursers/src/opencode.rs` require no source
  changes if compatibility re-exports are correct.
- The `core-compat` feature already enabled by `coursers-core` provides temporary TOML/file loading.
- The schema test prevents the known runtime/schema divergence from recurring.
- `cargo nextest run -p coursers-types --no-default-features` verifies the default types surface
  without relying on compatibility feature unification.

## Out Of Scope

- Changing source precedence or attaching provenance in current loaders.
- Trusting, denying, or authorizing actions based on provenance.
- Returning `HookExecution<T>` from current runtime paths.
- Moving filesystem, environment, Git, redaction, or subprocess I/O into adapters.
- Changing matching, rewrite, notification, redaction, or side-effect behavior.
- Changing Claude, Codex, or OpenCode response schemas or exit codes.
- Removing `core-compat`.

## Risk

- **Breaking API**: adding the `Redact` enum variant can break exhaustive external matches. Adding
  `HookContext.output` and `PipelineResult.replace_output` can break complete external struct
  literals and exhaustive patterns. These changes may land unreleased at workspace version 0.1.0,
  but must not ship in a 0.1.x release; the first published release containing them must bump the
  workspace to 0.2.0.
- **Serialization commitment**: newly emitted shapes become public contracts and require regression
  tests before future changes.
- **Security**: loaded provenance is output-only. Unknown `origin` fields on plain rules are ignored
  for compatibility and never become trusted metadata. Secret-bearing execution values do not
  implement Serde; only sanitized summaries do.
- **Compatibility**: old module paths and runtime signatures remain covered by compile and behavior
  tests.
- **Dependencies**: one development-only `static_assertions` dependency; optional production TOML
  compatibility remains unchanged.
- **Behavior**: no intended runtime behavior change in this task.
