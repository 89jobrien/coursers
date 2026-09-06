# Design: OpenCode Harness Support

## Goal

Add first-class OpenCode support that applies Coursers tool policies and lifecycle hook
pipelines through OpenCode's plugin API without coupling OpenCode to Claude or Codex protocol
semantics.

## Approved Approach

Use the approved first-class OpenCode bridge: a TypeScript plugin normalizes OpenCode events
for `crs hook --target opencode`, and the Rust CLI returns a harness-neutral result for the
plugin to apply.

## Context Map

### Files to Modify

| File                                                 | Purpose                    | Changes Needed                                                                                          |
| ---------------------------------------------------- | -------------------------- | ------------------------------------------------------------------------------------------------------- |
| `crates/coursers/src/opencode.rs`                    | OpenCode CLI adapter       | Add normalized request handling, neutral responses, hook-chain composition, and installation validation |
| `crates/coursers/src/lib.rs`                         | CLI dispatch               | Route `hook` and `validate-hooks` requests with target `opencode` to the adapter                        |
| `crates/coursers/src/crs_commands.rs`                | Generic lifecycle pipeline | Expose crate-private event parsing needed by the adapter                                                |
| `integrations/opencode/coursers.ts`                  | OpenCode plugin adapter    | Translate OpenCode plugin hooks and events to normalized Coursers requests                              |
| `integrations/opencode/opencode-plugin.d.ts`       | OpenCode type compatibility | Provide the compile-time subset of OpenCode 1.2.26 plugin and SDK declarations used by the adapter      |
| `integrations/opencode/coursers.test.ts`             | OpenCode plugin tests      | Verify tool normalization, result application, lifecycle mapping, and deduplication with a fake bridge  |
| `crates/coursers/tests/hook_opencode_integration.rs` | Process-level coverage     | Verify allow, deny, rewrite, filtering, and lifecycle responses                                         |
| `README.md`                                          | Installation overview      | Document OpenCode plugin installation and validation                                                    |
| `docs/opencode.md`                                   | Harness guide              | Document event mappings, behavior, limitations, and troubleshooting                                     |

### Dependencies

| File                                              | Relationship                                                                     |
| ------------------------------------------------- | -------------------------------------------------------------------------------- |
| `crates/core/src/hook/chain.rs`                   | Existing tool-policy ports and structured pre/post outcomes used unchanged       |
| `crates/core/src/hook/pipeline.rs`                | Existing lifecycle event model and generic pipeline used unchanged               |
| `crates/core/src/config.rs`                       | Existing `ProfileConfig::build_hook_chain` assembles tool policies and observers |
| `crates/core/src/hook/log.rs`                     | Existing matched-rule logging remains the pipeline audit sink                    |
| `crates/coursers/src/hook/chain_runner.rs`        | Reference for translating `HookChain` outcomes at a harness boundary             |
| `crates/coursers/tests/hook_codex_integration.rs` | Reference for target-specific process integration tests                          |

### Test Coverage

| Test                                              | Covers                                                             |
| ------------------------------------------------- | ------------------------------------------------------------------ |
| `crates/core/src/hook/chain.rs` inline tests      | Tool-chain ordering and short-circuit behavior                     |
| `crates/core/src/hook/pipeline.rs` inline tests   | Generic lifecycle matching, rewrites, notifications, and redaction |
| `crates/e2e/tests/hook_chain.rs`                  | Composed rule, rewrite, filter, and observer behavior              |
| `crates/coursers/tests/hook_codex_integration.rs` | Target-specific subprocess behavior                                |

The OpenCode protocol and event translation currently have no repository coverage; the new
integration test closes the Rust boundary gap. The TypeScript plugin will be validated by a
fixture-driven Bun test only when Bun is available, so Rust CI does not gain a new runtime
dependency.

### Reference Patterns

| File                                       | Pattern to Follow                                                          |
| ------------------------------------------ | -------------------------------------------------------------------------- |
| `crates/coursers/src/crs_commands.rs`      | Target dispatch, hook context construction, logging, and validation output |
| `crates/coursers/src/hook/chain_runner.rs` | Translation from structured core outcomes to harness responses             |
| `docs/codex-profile.md`                    | Harness-specific setup and protocol documentation                          |

### Risk

- No core crate API change is required; OpenCode is an adapter over existing ports.
- The neutral OpenCode response is new CLI JSON and therefore must be covered as a stable
  integration contract.
- OpenCode lifecycle events are not identical to Claude events; mappings must be explicit and
  avoid duplicate session or subagent stop delivery.
- OpenCode tool metadata is tool-specific. Missing shell exit metadata falls back to success,
  which prevents false failure-learning records but can miss a failure record.

## Crate Ownership

- **Owner crate**: `coursers` - harness payload parsing, CLI output, plugin validation, and
  external process integration belong at the binary adapter boundary.
- **Affected crates**: `coursers` only. `coursers-core` and `coursers-types` remain unchanged
  and are consumed through their existing public APIs.
- **Non-crate adapter**: `integrations/opencode/coursers.ts` owns OpenCode SDK integration and
  contains no policy logic.

## Public API

No new public Rust API is introduced. The existing CLI gains two supported invocations:

```text
crs hook --target opencode <event>
crs validate-hooks --target opencode
```

The OpenCode CLI JSON response is the public integration contract:

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
enum OpenCodeDecision {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct OpenCodeHookResponse {
    decision: OpenCodeDecision,
    reason: Option<String>,
    updated_input: Option<serde_json::Value>,
    replacement_output: Option<String>,
    messages: Vec<String>,
    matched_rules: Vec<String>,
}
```

### Internal Adapter API

The following crate-private API keeps CLI dispatch separate from protocol translation:

```rust
pub(crate) fn run_hook(
    event: coursers_core::hook_pipeline::HookEvent,
    raw_json: &str,
) -> miette::Result<OpenCodeHookResponse>;

pub(crate) fn write_hook_response(response: &OpenCodeHookResponse) -> miette::Result<()>;

pub(crate) fn validate_installation(
    home: &std::path::Path,
    current_dir: &std::path::Path,
) -> OpenCodeValidation;

pub(crate) fn cmd_validate_hooks();
```

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenCodeValidation {
    plugin_path: Option<std::path::PathBuf>,
    declaration_path: Option<std::path::PathBuf>,
    missing_binaries: Vec<String>,
}
```

The existing event parser becomes crate-private so both harness adapters use one accepted
event vocabulary:

```rust
pub(crate) fn parse_hook_event(
    event: &str,
) -> Option<coursers_core::hook_pipeline::HookEvent>;
```

No new trait is required. The existing `PreHook`, `PostHook`, and `Observer` traits are the
ports used by the OpenCode adapter.

## Event Mapping

| Coursers event      | OpenCode source                                          | Application behavior                                      |
| ------------------- | -------------------------------------------------------- | --------------------------------------------------------- |
| `PreToolUse`        | `tool.execute.before`                                    | Deny by throwing; apply rewrites to `output.args`         |
| `PostToolUse`       | `tool.execute.after`                                     | Replace `output.output` when filtering or redaction fires |
| `UserPromptSubmit`  | `chat.message`                                           | Deny by throwing; log non-blocking messages               |
| `PermissionRequest` | `permission.ask`                                         | Set `output.status` to `deny` on denial                   |
| `PreCompact`        | `experimental.session.compacting`                        | Run before compaction prompt generation                   |
| `PostCompact`       | `session.compacted`                                      | Run after successful compaction                           |
| `SessionStart`      | Root `session.created`                                   | Run once for a session without `parentID`                 |
| `SubagentStart`     | Child `session.created`                                  | Run once for a session with `parentID`                    |
| `Stop`              | Root `session.idle`                                      | Run when a root session becomes idle                      |
| `SubagentStop`      | Child `session.idle`                                     | Run when a child session becomes idle                     |
| `SessionEnd`        | `session.deleted`, plus tracked roots at plugin disposal | Run once per tracked root using in-memory deduplication   |

OpenCode tool names are normalized at the plugin boundary: `bash` becomes `Bash`, `edit`
becomes `Edit`, and `write` becomes `Write`. Tool input uses the existing Coursers keys,
including `command` for Bash and `file_path` for Edit/Write.

## Data Flow

1. Source: an OpenCode plugin hook or event supplies tool, prompt, permission, compaction, or
   session data to `integrations/opencode/coursers.ts`.
2. Transform: the plugin normalizes names and fields, then sends one JSON object to
   `crs hook --target opencode <event>` over stdin.
3. Transform: tool events run the existing `HookChain`; all events run the existing generic
   hook pipeline, with the first denial winning and later output replacement taking precedence.
4. Sink: the CLI writes exactly one `OpenCodeHookResponse` JSON object to stdout and writes
   diagnostics only to stderr.
5. Sink: the plugin applies denial, rewritten input, replacement output, and messages through
   the corresponding OpenCode hook API.

## Hexagonal Boundaries

- **Ports**: `PreHook`, `PostHook`, and `Observer` in
  `coursers_core::hook::chain`; generic lifecycle policy is represented by
  `coursers_core::hook_pipeline::run_pipeline`.
- **Rust adapter**: crate-private OpenCode target handling in
  `crates/coursers/src/opencode.rs` converts normalized JSON to existing core contexts and
  converts outcomes to `OpenCodeHookResponse`.
- **Harness adapter**: `integrations/opencode/coursers.ts` is the only component that imports
  OpenCode types or mutates OpenCode hook outputs.
- **Process boundary**: the plugin invokes `crs` directly with an argument array and JSON stdin;
  it does not construct a shell command string.

## Integration Points

- `Command::Hook` dispatches `target == "opencode"` to the new adapter before Claude/Codex
  handling.
- `Command::ValidateHooks` dispatches `target == "opencode"` to OpenCode installation
  validation.
- The validator requires `coursers.ts` and `opencode-plugin.d.ts` together in either the
  project-local `.opencode/plugins/` directory or global `$HOME/.config/opencode/plugins/`, and
  verifies `crs` and `opencode` are on PATH.
- The plugin maintains an in-memory map of session IDs to optional parent IDs and a set of
  delivered end events to classify child sessions and suppress duplicate lifecycle calls.
- No feature flag is needed because the adapter is dormant unless explicitly selected by the
  CLI target or installed as an OpenCode plugin.

## Failure Semantics

- Invalid normalized JSON or an unknown event returns a non-zero CLI exit and a diagnostic on
  stderr; the plugin reports the adapter failure and fails open unless a valid deny response was
  received.
- A policy denial returns exit zero with `decision: "deny"`; harness control flow belongs to
  the TypeScript adapter rather than process exit semantics.
- For Bash post-tool events, `metadata.exit` supplies `exit_code` when it is numeric. Missing
  metadata uses zero so failure learning does not record an unverified failure.
- Lifecycle actions that cannot block the originating OpenCode event still execute side effects
  and surface messages through structured OpenCode application logging.

## Installation and Validation

The repository ships the plugin source and compatibility declaration but does not mutate
user-global configuration. Users copy or symlink exactly `coursers.ts` and
`opencode-plugin.d.ts` into either supported OpenCode plugin directory, then run:

```sh
crs validate-hooks --target opencode
```

Validation reports the discovered plugin and declaration paths plus missing binaries. It does
not execute hooks or modify OpenCode configuration.

## Out of Scope

- Publishing the bridge as an npm package.
- Automatically writing files under `$HOME/.config/opencode`.
- Persisting OpenCode session relationship state across plugin restarts.
- Treating `session.idle` as `SessionEnd`; idle sessions remain resumable.
- Injecting Coursers notifications into model context; notifications use application logging.
- Adding OpenCode-specific policy rules or a separate OpenCode profile.
- Depending on experimental OpenCode v2-only event names when stable plugin hooks provide an
  equivalent event.

## Doublecheck

- [x] Ownership stays in the `coursers` adapter crate; core policy types remain harness-neutral.
- [x] No circular dependency is introduced; the existing `coursers` to `coursers-core` edge is
      unchanged.
- [x] The API surface is minimal: one neutral response contract and no new Rust trait.
- [x] OpenCode is isolated in a TypeScript adapter; no OpenCode dependency enters the Rust
      workspace.
- [x] Event and field names match the OpenCode plugin types used by installed OpenCode 1.2.26,
      with explicit fallbacks for tool-specific metadata.

## Risk Summary

- [ ] Breaking API changes: no; existing targets and defaults remain unchanged.
- [ ] New external dependency: no Rust dependency; runtime use requires the already-installed
      OpenCode plugin runtime and `crs` binary.
- [ ] Feature flag required: no.
- [x] Version-sensitive integration: yes; OpenCode experimental compaction hooks and tool
      metadata require integration tests and documentation of graceful fallback behavior.
