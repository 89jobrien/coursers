# coursers-core

`coursers-core` contains Coursers policy and domain behavior: rule matching, failure learning,
filtering, rewriting, generic hook pipelines, history analysis, replay, and persistence adapters.
It sits between the dependency-light contracts in `coursers-types` and the protocol/process adapters
in the `coursers` package.

The workspace policy requires Rust 1.89 or newer and uses Rust 2024. This package inherits the
edition, but its manifest currently does not inherit the workspace `rust-version`. It is licensed
under MIT OR Apache-2.0.

## Workspace role

The crate owns decisions, parsing, and composition. The CLI crate owns stdin/stdout protocols and
command presentation. Some filesystem-backed implementations remain here for compatibility while
the workspace moves stable contracts into `coursers-types`.

| Area | Important modules |
| --- | --- |
| Rules and state | `rules`, `state`, `loader`, `store`, `config` |
| Hook behavior | `hook::chain`, `hook::concrete`, `hook::pipeline`, `hook::protocol` |
| Output control | `hook::filters`, `hook::filter_logic`, `hook::rewrite`, `hook::tool_swap` |
| Parsing | `parse::ast`, `parse::pipeline`, `parse::expand` |
| Analysis | `analyze::{history, capture, stats, suggest, insights, heat}` |
| Integrations | `jsonl_source`, `rtk`, `obfsck`, `rx_prefix` |
| Diagnostics | `error`, `diagnostics`, `replay`, `date` |

The crate re-exports historical module paths such as `coursers_core::filters` and
`coursers_core::pipeline`. It also re-exports `coursers_types` as `coursers_core::types`. Preserve
those paths when changing internals unless a deliberate compatibility release removes them.

## Using the crate

```toml
[dependencies]
coursers-core = { path = "../core" }
```

The most direct policy API checks a command against a rule list:

```rust
use coursers_core::rules::{Rule, check_pipeline};

let rules = vec![Rule {
    id: "no-grep".into(),
    enabled: true,
    pattern: r"\bgrep\b".into(),
    pattern_flags: String::new(),
    exceptions: vec![],
    target_commands: vec!["grep".into()],
    message: Some("Use the Grep tool.".into()),
    task_override: None,
}];

assert!(check_pipeline("printf x | grep x", &rules).is_some());
```

For application wiring, prefer injected loaders and stores over direct file access:

```rust
use coursers_core::config::ConfigBuilder;
use coursers_core::hook::chain::{HookContext, PreHookOutcome};
use serde_json::json;

let profile = ConfigBuilder::new().build();
let chain = profile.build_hook_chain();
let context = HookContext::new("Bash", json!({"command": "cargo check"}));
let outcome = chain.run_pre(&context)?;
assert!(matches!(outcome, PreHookOutcome::Allow));
# Ok::<(), coursers_core::CourserError>(())
```

## Key APIs and behavior

### Rule evaluation

- `rules::check` evaluates the full command and returns the first matching rule and message.
- `rules::check_pipeline` checks sequential `&&`, `||`, and `;` segments, then falls back to the
  whole command so operator-matching rules and whole-command exceptions still work.
- `rules::check_pipeline_with_task_overrides` skips matching rules overridden by running task
  titles but continues scanning later rules.
- `rules::check_with_span` also returns the matched byte range for `RuleViolation` diagnostics.
- Invalid rule or exception regexes do not panic. A malformed rule pattern cannot match; a malformed
  exception cannot exempt a command.

Target-command gating parses each pipe stage and compares its first argument. This narrows broad
regexes to actual command positions and enriches deny messages with the offending stage.

### Failure learning

`state::record_failure` keeps timestamp windows bounded by `FailureLearning`; `state::check_learned`
returns a deny message after the configured threshold. Commands are keyed by a SHA-256 digest rather
than used directly as map keys. `StateStore` separates persistence from policy.

`FsStateStore` writes JSON through a same-directory temporary file and rename. The current load,
mutate, and save sequence is not a cross-process transaction, so callers must not claim concurrent
updates are lossless. Missing or malformed state is commonly treated as empty by hook adapters to
keep hooks available.

### Composable hook chain

`hook::chain` defines three ports:

- `PreHook`: first `Deny` or `Rewrite` wins and short-circuits later pre-hooks.
- `PostHook`: every hook runs and the last non-`Allow` filter outcome wins.
- `Observer`: every observer runs; observer errors are reported and remain non-fatal.

`ProfileConfig::build_hook_chain` wires `RuleBlockHook`, `RewriteHook`, `FilterHook`, and
`FailureObserver` in that order. The CLI currently enables this path only when
`COURSERS_HOOK_CHAIN=1`; the legacy pre/post path remains the default compatibility behavior.

### Filters and rewrites

Filter and rewrite configuration resolves in this order:

1. `CRS_FILTERS`, which is an exclusive override.
2. Project `.ctx/crs-filters.toml`, found by walking upward from the current directory.
3. Global `$HOME/.config/crs/filters.toml`.

The legacy filter/rewrite loaders merge project content before global content. Filter matching uses
the first matching rule, while rewrite application folds through every matching rule in file order.
Each rewrite rule is evaluated once against the command produced so far.

The profile-backed chain adapters are not fully symmetrical today: `ProfileFsFiltersLoader` selects
the project file instead of merging it with the global file, while `ProfileFsRewriteLoader` merges
both. Preserve this observed behavior in compatibility work or change it with explicit regression
tests and release notes.

`parse::expand::EnvExpander` expands supported environment syntax before rewriting. The injected
`NoopExpander` is useful when an adapter must preserve the command exactly.

### Generic TOML hook pipeline

`hook::pipeline` loads global hooks, sorted plugin files, then project-local hooks:

```text
$HOME/.config/crs/hooks.toml
$HOME/.config/crs/plugins.d/*.toml
.ctx/crs-hooks.toml
```

Rules are evaluated in order. The first deny returns immediately; rewrites, notifications, captured
run output, and redaction results otherwise accumulate. Runtime actions are `deny`, `rewrite`,
`run`, `notify`, and `redact`. The checked-in JSON schema currently documents the first four, so
runtime and schema must be updated together when this contract changes.

`run` actions execute real child processes. Project-local hook configuration is therefore trusted
code, not a harmless formatting file. `redact` invokes `obfsck redact` and fails open by preserving
the original output if the binary cannot run.

### Protocol output

`hook::protocol` centralizes response JSON. Claude deny responses use exit code 2; Codex deny
responses use exit code 0 with a structured decision. Rewrites are allow responses with
`updatedInput.command`. Post-tool filtering uses a result envelope. Keep protocol JSON on stdout and
send diagnostics to stderr.

## Configuration paths

| Purpose | Resolution |
| --- | --- |
| Static rules | `COURSERS_RULES`, otherwise `$HOME/.config/coursers/course-correct-rules.json` |
| Failure state | `COURSERS_STATE`, local `.ctx` state if present, otherwise global state |
| Named profile | `$HOME/.config/coursers/profiles/<name>/` |
| Filters/rewrites | `CRS_FILTERS`, project `.ctx`, then global `$HOME/.config/crs` |
| Generic hooks | Global hooks, sorted plugins, then project `.ctx/crs-hooks.toml` |

Configuration helpers generally fail open for missing optional files. Explicit profile loaders
return typed `CourserError` values for I/O, JSON, or TOML failures so application boundaries can
choose whether to warn, deny, or continue.

## Compatibility and safety contracts

- Keep persisted JSON and TOML shapes backward compatible; use defaults for additive fields.
- Do not emit secrets, complete hook payloads, or unredacted environment values in diagnostics.
- Do not move protocol formatting into policy functions; harness exit codes differ.
- Preserve the documented loader-specific project/global behavior and the `CRS_FILTERS` exclusive
  override semantics.
- Preserve first-match rule blocking and sequential rewrite semantics unless tests and release notes
  explicitly define a migration.
- Keep external I/O behind loaders, stores, or client traits. Tests should inject in-memory or
  tempdir-backed implementations.
- `coursers-core` uses `#![warn(unreachable_pub)]`; new public items must belong to the supported
  API.

## Development and testing

The `testing` feature exposes in-memory adapters and enables conformance/property test targets:

```sh
cargo check -p coursers-core --all-features
cargo clippy -p coursers-core --all-targets --all-features -- -D warnings
cargo nextest run -p coursers-core --features testing
cargo nextest run -p coursers-e2e
cargo xtask protocol drift
```

Changes to ports, serialized contracts, or rule behavior require downstream `coursers` and E2E
tests. Focused tests live beside modules; `crates/core/tests` verifies loader/store/client
conformance and property invariants.
