# Plan: OpenCode Harness Support

## Goal

Add first-class OpenCode support that applies Coursers tool policies and lifecycle pipelines
through a typed OpenCode plugin and a harness-neutral `crs` JSON boundary.

## Context Map

### Files to Modify

| File | Role |
| --- | --- |
| `crates/coursers/src/opencode.rs` | OpenCode request evaluation, neutral responses, and installation validation |
| `crates/coursers/src/lib.rs` | Module declaration and target dispatch |
| `crates/coursers/src/crs_commands.rs` | Shared crate-private event parser |
| `crates/coursers/tests/hook_opencode_integration.rs` | CLI contract tests |
| `integrations/opencode/coursers.ts` | OpenCode plugin adapter |
| `integrations/opencode/coursers.test.ts` | Plugin unit tests with an injected bridge |
| `README.md` | Installation summary |
| `docs/opencode.md` | Complete OpenCode setup and behavior guide |

### Dependencies and Reference Patterns

- `crates/core/src/hook/chain.rs` provides `HookChain`, `HookContext`, `PreHookOutcome`,
  `PostHookOutcome`, and `ToolOutput` without modification.
- `crates/core/src/hook/pipeline.rs` provides `HookEvent`, `HookContext`, `PipelineResult`,
  `load_config`, and `run_pipeline` without modification.
- `crates/core/src/config.rs` provides `ConfigBuilder::build` and
  `ProfileConfig::build_hook_chain` without modification.
- `crates/coursers/src/hook/chain_runner.rs` is the outcome-translation reference.
- `crates/coursers/tests/hook_codex_integration.rs` is the subprocess-test reference.

### Risk

- [ ] No breaking Rust API change; only the accepted CLI target set grows.
- [ ] The new neutral JSON response is a stable external contract and needs exact assertions.
- [ ] Lifecycle mappings need in-memory deduplication to avoid repeated stop/end actions.
- [ ] Missing OpenCode shell exit metadata must default to zero to avoid false failure records.
- [ ] Experimental compaction hooks need graceful isolation in the TypeScript adapter.

## Architecture

- Crates affected: `coursers` only; `coursers-core` remains an unchanged dependency.
- New Rust types in `crates/coursers/src/opencode.rs`: `OpenCodeDecision`,
  `OpenCodeHookResponse`, and `OpenCodeValidation`.
- New TypeScript types in `integrations/opencode/coursers.ts`: `BridgeRequest`,
  `BridgeResponse`, `Bridge`, and `SessionState`.
- Data flow: OpenCode hook -> TypeScript normalization -> JSON stdin -> existing Coursers
  chain/pipeline -> neutral JSON stdout -> OpenCode mutation, denial, or log sink.
- Hexagonal boundary: existing core `PreHook`, `PostHook`, and `Observer` traits remain the
  ports; Rust target handling and the TypeScript plugin are adapters.

## Tech Stack

- Rust 2024, `serde`, `serde_json`, `miette`, existing `coursers-core` APIs.
- TypeScript executed by OpenCode's Bun runtime and typed by `@opencode-ai/plugin`.
- No new Cargo or npm dependency.
- Rust tests use `cargo nextest`; plugin tests use `bun test` when Bun is available.

## Tasks

### Task 1: Define the neutral OpenCode response contract

**Crate**: `coursers`
**File(s)**: `crates/coursers/src/opencode.rs`, `crates/coursers/src/lib.rs`
**Run**: `cargo nextest run -p coursers -E 'test(opencode_response_serializes_stable_contract)'

1. Add `pub mod opencode;` to `crates/coursers/src/lib.rs` and create
   `crates/coursers/src/opencode.rs` with this failing serialization test:

   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;
       use serde_json::json;

       #[test]
       fn opencode_response_serializes_stable_contract() {
           let response = OpenCodeHookResponse {
               decision: OpenCodeDecision::Allow,
               reason: None,
               updated_input: Some(json!({"command": "nu -c ls"})),
               replacement_output: None,
               messages: vec!["rewritten".to_string()],
               matched_rules: vec!["prefer-nu".to_string()],
           };
           assert_eq!(
               serde_json::to_value(response).unwrap(),
               json!({
                   "decision": "allow",
                   "reason": null,
                   "updated_input": {"command": "nu -c ls"},
                   "replacement_output": null,
                   "messages": ["rewritten"],
                   "matched_rules": ["prefer-nu"]
               })
           );
       }
   }
   ```

   Run the task command. Expected: FAIL because the response types do not exist.

2. Add these exact contract types and constructors above the test:

   ```rust
   use serde::Serialize;
   use serde_json::Value;

   #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
   #[serde(rename_all = "kebab-case")]
   enum OpenCodeDecision {
       Allow,
       Deny,
   }

   #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
   struct OpenCodeHookResponse {
       decision: OpenCodeDecision,
       reason: Option<String>,
       updated_input: Option<Value>,
       replacement_output: Option<String>,
       messages: Vec<String>,
       matched_rules: Vec<String>,
   }

   impl Default for OpenCodeHookResponse {
       fn default() -> Self {
           Self {
               decision: OpenCodeDecision::Allow,
               reason: None,
               updated_input: None,
               replacement_output: None,
               messages: Vec::new(),
               matched_rules: Vec::new(),
           }
       }
   }
   ```

3. Run `cargo nextest run -p coursers -E 'test(opencode_response_serializes_stable_contract)'`,
   then `cargo clippy -p coursers --all-targets -- -D warnings`.

4. Run `git branch --show-current`; verify it prints `main`. Commit with
   `git commit -m "feat(coursers): define OpenCode hook response contract"`.

### Task 2: Evaluate OpenCode tool requests through existing ports

**Crate**: `coursers`
**File(s)**: `crates/coursers/src/opencode.rs`, `crates/coursers/tests/hook_opencode_integration.rs`
**Run**: `cargo nextest run -p coursers -E 'test(opencode_tool_)'`

1. Create `crates/coursers/tests/hook_opencode_integration.rs` using
   `#[path = "common_bin.rs"] mod common_bin;`. Add a helper that starts `crs` with arguments
   `hook --target opencode <event>`, writes JSON to stdin, and captures stdout/stderr. Add these
   tests with exact names and assertions:

   - `opencode_tool_allow_returns_neutral_json`: empty temporary rules and filters produce exit
     zero and `{"decision":"allow"}` with null mutation fields.
   - `opencode_tool_deny_returns_zero_with_deny_decision`: a temporary rule matching
     `rm -rf` produces exit zero, `decision == "deny"`, and a non-empty `reason`.
   - `opencode_tool_rewrite_returns_updated_input`: a temporary `[[rewrites]]` rule replacing
     `^ls$` with `eza` produces `updated_input.command == "eza"`.
   - `opencode_tool_filter_returns_replacement_output`: a temporary truncate filter produces a
     non-null `replacement_output` for a post-tool request.

   Each request must use the normalized shape:

   ```json
   {
     "tool_name": "Bash",
     "tool_input": {"command": "ls"},
     "tool_response": {"exit_code": 0, "output": "one\ntwo\nthree"},
     "session_id": "ses-root"
   }
   ```

   Set `COURSERS_RULES`, `COURSERS_STATE`, and `CRS_FILTERS` to temporary files. Run the task
   command. Expected: FAIL because target `opencode` is unknown.

2. Implement these crate-private functions in `crates/coursers/src/opencode.rs`:

   ```rust
   pub(crate) fn run_hook(
       event: coursers_core::hook_pipeline::HookEvent,
       raw_json: &str,
   ) -> miette::Result<OpenCodeHookResponse>;

   pub(crate) fn write_hook_response(response: &OpenCodeHookResponse) -> miette::Result<()>;
   ```

   `run_hook` must deserialize `raw_json` to `serde_json::Value`, build the profile with
   `coursers_core::config::ConfigBuilder::new().build()`, and use
   `ProfileConfig::build_hook_chain()` only for Bash `PreToolUse` and `PostToolUse` events.
   Build `coursers_core::hook::chain::HookContext` from canonical `tool_name` and the cloned
   `tool_input` object. For post-tool execution, build `ToolOutput` from
   `tool_response.output` and numeric `tool_response.exit_code`, defaulting each to empty text
   and zero respectively.

3. Translate outcomes exactly:

   ```rust
   match chain.run_pre(&context)? {
       PreHookOutcome::Allow => {}
       PreHookOutcome::Deny(reason) => {
           response.decision = OpenCodeDecision::Deny;
           response.reason = Some(reason);
       }
       PreHookOutcome::Rewrite { command, reason } => {
           let mut input = tool_input.clone();
           input["command"] = Value::String(command);
           response.updated_input = Some(input);
           response.messages.push(reason);
       }
   }
   ```

   Translate `PostHookOutcome::Filter(text)` to `replacement_output = Some(text)`. Always emit
   exactly one JSON line. Policy denials are successful evaluations and therefore exit zero.

4. Run the task command, `cargo nextest run -p coursers`, and
   `cargo clippy -p coursers --all-targets -- -D warnings`.

5. Run `git branch --show-current`; verify `main`. Commit with
   `git commit -m "feat(coursers): evaluate OpenCode tool hooks"`.

### Task 3: Add generic lifecycle evaluation and CLI dispatch

**Crate**: `coursers`
**File(s)**: `crates/coursers/src/opencode.rs`, `crates/coursers/src/crs_commands.rs`, `crates/coursers/src/lib.rs`, `crates/coursers/tests/hook_opencode_integration.rs`
**Run**: `cargo nextest run -p coursers -E 'test(opencode_lifecycle_)'`

1. Add integration tests:

   - `opencode_lifecycle_notify_returns_messages`: set a temporary `.ctx/crs-hooks.toml` with
     a `session-start` notify rule, run from that temporary project, and assert the message and
     matched rule are present.
   - `opencode_lifecycle_deny_is_structured`: configure a `user-prompt-submit` deny rule and
     assert exit zero plus `decision == "deny"`.
   - `opencode_invalid_json_exits_nonzero`: pass `{`, assert failure and a non-empty stderr.

   Run the task command. Expected: FAIL because generic pipeline results are not merged.

2. Change `parse_hook_event` in `crates/coursers/src/crs_commands.rs` to
   `pub(crate) fn parse_hook_event(...)`. In `Command::Hook` dispatch, parse the event once and,
   when `target == "opencode"`, read stdin into a string, call `opencode::run_hook`, and call
   `opencode::write_hook_response`. Preserve current Claude and Codex paths unchanged.

3. In `run_hook`, construct `coursers_core::hook_pipeline::HookContext` with:

   ```rust
   HookContext {
       event: Some(event),
       tool_name,
       target,
       exit_code,
       raw_json: Some(raw_json.to_string()),
       output,
   }
   ```

   Derive `target` from `tool_input.command`, then `tool_input.file_path`, then top-level
   `target`. Run `load_config()` and `run_pipeline()`. If the response is not already denied,
   copy pipeline denial into `decision` and `reason`. Merge pipeline rewrites into a cloned
   `tool_input`, replacement output into `replacement_output`, and append `messages` and
   `matched_rules`. A generic replacement output overrides an earlier chain filter.

4. Record a hook-log entry only when `matched_rules` is non-empty, using
   `coursers_core::hook::log::entry_from_pipeline` and `record` with the generic context and
   pipeline result.

5. Run the task command, the complete `coursers` test package, and clippy.

6. Run `git branch --show-current`; verify `main`. Commit with
   `git commit -m "feat(coursers): route OpenCode lifecycle hooks"`.

### Task 4: Validate OpenCode plugin installation

**Crate**: `coursers`
**File(s)**: `crates/coursers/src/opencode.rs`, `crates/coursers/src/lib.rs`, `crates/coursers/tests/hook_opencode_integration.rs`
**Run**: `cargo nextest run -p coursers -E 'test(opencode_validation_)'`

1. Add tests with exact names:

   - `opencode_validation_accepts_project_plugin`: create
     `.opencode/plugins/coursers.ts` under a temporary current directory and assert validation
     reports that path.
   - `opencode_validation_accepts_global_plugin`: create
     `.config/opencode/plugins/coursers.ts` under a temporary home and assert validation reports
     that path.
   - `opencode_validation_reports_missing_plugin`: create neither path and assert no plugin path.

   Run the task command. Expected: FAIL because validation does not exist.

2. Add the designed type and functions:

   ```rust
   #[derive(Debug, Clone, PartialEq, Eq)]
   struct OpenCodeValidation {
       plugin_path: Option<std::path::PathBuf>,
       missing_binaries: Vec<String>,
   }

   pub(crate) fn validate_installation(
       home: &std::path::Path,
       current_dir: &std::path::Path,
   ) -> OpenCodeValidation;

   pub(crate) fn cmd_validate_hooks();
   ```

   Prefer the project plugin path, then the global path. Check `crs` and `opencode` with
   `std::process::Command::new("which")`. Print the discovered path and missing items;
   `cmd_validate_hooks` exits one if the plugin or either binary is absent.

3. Route `Command::ValidateHooks { target }` using an exhaustive string match:

   ```rust
   match target.as_str() {
       "claude" => crs_commands::cmd_validate_hooks(),
       "codex" => crs_commands::cmd_validate_codex_hooks(),
       "opencode" => opencode::cmd_validate_hooks(),
       other => {
           eprintln!("crs validate-hooks: unknown target '{other}'");
           std::process::exit(1);
       }
   }
   ```

4. Run the task command and all `coursers` tests and clippy.

5. Run `git branch --show-current`; verify `main`. Commit with
   `git commit -m "feat(coursers): validate OpenCode plugin setup"`.

### Task 5: Implement and test OpenCode tool hooks

**Crate**: `coursers`
**File(s)**: `integrations/opencode/coursers.ts`, `integrations/opencode/coursers.test.ts`
**Run**: `bun test integrations/opencode/coursers.test.ts`

1. Create a failing Bun test that imports `createCoursersPlugin` and injects a fake `Bridge`.
   Cover these exact test names:

   - `normalizes bash input and applies rewritten command`
   - `throws the policy reason when a tool is denied`
   - `uses metadata exit and replaces post-tool output`
   - `sets permission status to deny`

   The fake bridge records `{ event, payload }` calls and returns queued `BridgeResponse`
   objects. Run the task command. Expected: FAIL because the plugin module does not exist.

2. Define these types and export the factory from `integrations/opencode/coursers.ts`:

   ```typescript
   import type { Plugin } from "@opencode-ai/plugin"

   export type BridgeRequest = {
     tool_name?: string
     tool_input?: Record<string, unknown>
     tool_response?: { exit_code: number; output: string }
     session_id?: string
     target?: string
   }

   export type BridgeResponse = {
     decision: "allow" | "deny"
     reason: string | null
     updated_input: Record<string, unknown> | null
     replacement_output: string | null
     messages: string[]
     matched_rules: string[]
   }

   export type Bridge = (
     event: string,
     payload: BridgeRequest,
   ) => Promise<BridgeResponse>

   export function createCoursersPlugin(bridge?: Bridge): Plugin;
   export const CoursersPlugin: Plugin;
   ```

3. The default bridge must invoke Bun with an argument array, never a shell string:

   ```typescript
   const process = Bun.spawn(
     ["crs", "hook", "--target", "opencode", event],
     { cwd: directory, stdin: JSON.stringify(payload), stdout: "pipe", stderr: "pipe" },
   )
   ```

   Await the exit code and both streams. On non-zero or malformed stdout, log through
   `client.app.log` and return an allow response. On a valid response, log every message through
   `client.app.log` at `info` level.

4. Implement `tool.execute.before`, `tool.execute.after`, `chat.message`,
   `permission.ask`, and `experimental.session.compacting`. Normalize `bash`, `edit`, and
   `write` to `Bash`, `Edit`, and `Write`; normalize `filePath` to `file_path`. Apply
   `updated_input` with `Object.assign`, apply `replacement_output` directly, throw on tool or
   prompt denial, and set permission output status to `deny` on permission denial.

5. Run the task command. Then run
   `bunx tsc --noEmit --moduleResolution bundler --module preserve --target es2022 integrations/opencode/coursers.ts`.

6. Run `git branch --show-current`; verify `main`. Commit with
   `git commit -m "feat(opencode): bridge tool hooks to Coursers"`.

### Task 6: Map and deduplicate OpenCode lifecycle events

**Crate**: `coursers`
**File(s)**: `integrations/opencode/coursers.ts`, `integrations/opencode/coursers.test.ts`
**Run**: `bun test integrations/opencode/coursers.test.ts`

1. Add failing tests with exact names:

   - `maps root creation and idle to session start and stop`
   - `maps child creation and idle to subagent start and stop`
   - `maps compaction hooks before and after compaction`
   - `delivers session end once across deletion and disposal`

   Use synthetic `session.created`, `session.idle`, `session.compacted`, and `session.deleted`
   events with root and child IDs. Run the task command. Expected: FAIL because lifecycle state
   is not implemented.

2. Add this state type and initialize it per plugin instance:

   ```typescript
   type SessionState = {
     parents: Map<string, string | undefined>
     ended: Set<string>
   }
   ```

3. In the plugin `event` hook, apply this exact mapping:

   - Root `session.created` -> `session-start`.
   - Child `session.created` -> `subagent-start`.
   - Known root `session.idle` -> `stop`.
   - Known child `session.idle` -> `subagent-stop`.
   - `session.compacted` -> `post-compact`.
   - Root `session.deleted` -> `session-end`, then mark ended and remove from `parents`.
   - Child `session.deleted` -> remove from `parents` without a second subagent-stop.

   Include `session_id`, `target`, and the original event properties in each normalized payload.
   The direct `experimental.session.compacting` hook emits `pre-compact`.

4. In `dispose`, emit `session-end` for every tracked root not present in `ended`. Add each ID
   to `ended` before awaiting the bridge call so concurrent disposal cannot duplicate delivery.

5. Run the task command and the TypeScript check from Task 5.

6. Run `git branch --show-current`; verify `main`. Commit with
   `git commit -m "feat(opencode): map full hook lifecycle"`.

### Task 7: Document installation, mappings, and limitations

**Crate**: `coursers`
**File(s)**: `README.md`, `docs/opencode.md`
**Run**: `git diff --check -- README.md docs/opencode.md`

1. Before editing, run
   `rg -n "validate-hooks --target opencode|tool.execute.before" README.md docs/opencode.md`.
   Expected: FAIL with no matches.

2. Add an OpenCode paragraph after the Codex paragraph in `README.md`. Include project and global
   plugin destinations, the source `integrations/opencode/coursers.ts`, and
   `crs validate-hooks --target opencode`.

3. Create `docs/opencode.md` with these complete sections: Requirements, Installation, Event
   Mapping, Neutral JSON Contract, Failure Semantics, Validation, and Troubleshooting. Copy the
   approved event table from
   `docs/designs/2026-09-04-opencode-harness-support-design.md`. State explicitly that
   `session.idle` maps to Stop rather than SessionEnd, SessionEnd is best effort on deletion or
   plugin disposal, missing `metadata.exit` defaults to zero, adapter failures fail open, and
   notifications go to OpenCode application logs rather than model context.

4. Run the task command and rerun the `rg` command; verify both documents match.

5. Run `git branch --show-current`; verify `main`. Commit with
   `git commit -m "docs: add OpenCode harness setup"`.

### Task 8: Run workspace verification

**Crate**: `coursers`
**File(s)**: `crates/coursers/src/opencode.rs`, `crates/coursers/src/lib.rs`, `crates/coursers/src/crs_commands.rs`, `crates/coursers/tests/hook_opencode_integration.rs`, `integrations/opencode/coursers.ts`, `integrations/opencode/coursers.test.ts`, `README.md`, `docs/opencode.md`
**Run**: `cargo check --workspace`

1. Add the regression test `opencode_target_does_not_change_claude_default` to
   `crates/coursers/tests/hook_opencode_integration.rs`. Invoke `crs hook session-start` without
   `--target` and assert it retains the existing Claude response behavior. Run only that test;
   it must pass before any cleanup, proving the default did not regress.

2. Run, in order:

   ```sh
   cargo fmt --all --check
   cargo check --workspace
   cargo clippy --workspace --all-targets -- -D warnings
   cargo nextest run --workspace
   bun test integrations/opencode/coursers.test.ts
   git diff --check
   ```

   If Bun is unavailable, record that single skipped gate in the handoff; do not install a new
   runtime as part of this task.

3. Run `just install`, then verify the installed binary with:

   ```sh
   crs hook --target opencode session-start < /dev/null
   crs validate-hooks --target opencode
   ```

   The first command must emit one allow response. The validation command may report a missing
   installed plugin if the user has not linked it; that is an environment result, not a test
   failure. Do not write to `$HOME/.config/opencode`.

4. Run `git status --short --branch` and inspect the complete diff. Do not include unrelated
   user changes.

5. Run `git branch --show-current`; verify `main`. Commit the regression test or formatting-only
   cleanup, if any, with `git commit -m "test(coursers): verify OpenCode harness integration"`.
   If the task creates no diff, do not create an empty commit.

## Requirement Traceability

| Requirement | Tasks |
| --- | --- |
| Neutral OpenCode target | 1-3 |
| Tool blocking, rewriting, filtering, and failure learning | 2, 5 |
| Prompt and permission handling | 3, 5 |
| Compaction, session, and subagent lifecycle | 3, 6 |
| Installation validation | 4 |
| Plugin and CLI regression tests | 2-6, 8 |
| User documentation | 7 |

## Pre-Save Checklist

- [x] Every approved requirement maps to at least one task.
- [x] Every task names exact paths and final type/function names.
- [x] Production changes follow a failing test or failing documentation check.
- [x] Tasks form one sequential dependency chain and each implementation task ends with a commit.
- [x] Existing Claude and Codex behavior remains explicitly covered and unchanged.
