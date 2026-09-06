# AGENTS.md

Guidance for coding agents working in this Rust workspace. Prefer repository evidence over stale
memory, preserve unrelated user changes, and keep edits scoped to the requested task.

## Workflow Discipline

This repository uses OPAVS (Orient, Plan, Act, Verify, Ship).

- Check the phase with `opavs phase get`; change it with `opavs phase set <PHASE>`.
- Manage `.ctx/opavs/tasks.yaml` through `opavs tasks`, not by hand.
- Read `.ctx/opavs/memory-bank/active-context.md` and `progress.md` when resuming work.
- ORIENT and PLAN are read-only; edit only in ACT, test in VERIFY, commit/push only in SHIP.
- `.ctx/opavs/phase` is local and intentionally uncommitted.
- Run `git status --short --branch` before editing; never discard changes you did not create.

## Workspace Ownership

The workspace uses Rust 2024, resolver 3, and a minimum Rust version of 1.89.

- `crates/types` (`coursers-types`): domain data and stable port traits.
- `crates/core` (`coursers-core`): rules, parsing, state, filters, rewrites, analysis, and hooks.
- `crates/coursers` (`coursers`): CLI dispatch, concrete adapters, protocol I/O, and both binaries.
- `crates/e2e` (`coursers-e2e`): realistic cross-command and hook-chain scenarios.
- `crates/xtask` (`xtask`): taskit-backed workspace quality gates.
- Put external I/O behind traits and inject fakes or tempdir-backed adapters in tests.
- Do not mock the filesystem when an existing port (`RulesLoader`, `StateStore`, etc.) fits.

There is no separate `crs` crate. `crates/coursers/Cargo.toml` defines `coursers` from
`src/main.rs` and `crs` from the thin `src/bin/crs.rs`; both call `coursers::run` and expose the
same Clap command model. Check the entrypoints before assuming behavior is binary-specific.

## Build and Install

```sh
cargo check --workspace                 # fastest compile validation
cargo build                             # debug workspace build
cargo build --release                   # optimized, stripped/LTO build
just check                              # workspace check + clippy
just install                            # install coursers and crs to $HOME/.cargo/bin
```

`cargo build` and tests do not refresh installed binaries. After changing `crates/core` or
`crates/coursers`, run `just install` before testing live `crs`/`coursers` hook behavior.

## Formatting and Linting

```sh
cargo fmt --all                         # apply rustfmt
cargo fmt --all -- --check              # verify formatting only
cargo clippy --workspace -- -D warnings
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Use the broader all-targets/all-features clippy command before shipping. Test-only warnings have
repeatedly escaped narrower lint runs. CI sets `RUSTFLAGS=-D warnings`.

## Tests

Prefer nextest; use `cargo test` only when nextest is unavailable or libtest-specific flags are
needed.

```sh
cargo nextest run --workspace
cargo nextest run -p coursers-types
cargo nextest run -p coursers-core
cargo nextest run -p coursers
cargo nextest run -p coursers-e2e
cargo xtask ci --fail-fast              # same top-level pipeline used by CI
nu scripts/smoke.nu                     # installed/release binary smoke test
bun test integrations/opencode/coursers.test.ts
```

### Run One Test

```sh
# Name/substring filter within one package
cargo nextest run -p coursers-core -E 'test(rule_name)'

# One integration-test target, then one exact test in it
cargo nextest run -p coursers --test hook_opencode_integration \
  -E 'test(opencode_tool_deny_returns_zero_with_deny_decision)'

# Libtest alternative; use the fully qualified test path with --exact
cargo test -p coursers-core 'rules::tests::<test_name>' -- --exact --nocapture

# One complete integration-test target under libtest
cargo test -p coursers --test hook_opencode_integration
```

Start with the narrowest relevant test, then run the owning package, workspace check, broad clippy,
and affected end-to-end tests. Changes to shared ports or core behavior require downstream tests.

## Rust Style

- Let `rustfmt` decide layout; do not hand-align fields or fight automatic wrapping.
- Keep modules focused and `main.rs` entrypoints thin; move behavior into library modules.
- Group imports by origin with blank lines where useful: `std`, external crates, then `crate`/`super`.
- Prefer explicit imports over glob imports outside test modules; use `Trait as _` for method lookup
  when the trait name itself is intentionally unused.
- Use `UpperCamelCase` for types/traits/enums, `snake_case` for modules/functions/variables, and
  `SCREAMING_SNAKE_CASE` for constants/statics.
- Name tests after observable behavior, for example `opencode_tool_deny_returns_zero_with_deny_decision`.
- Prefer `&str`, slices, and borrowed values when ownership is unnecessary; avoid reflexive clones.
- Model optional values with `Option`, recoverable failures with `Result`, and modes with enums
  rather than bool flags or magic strings.
- Derive `Debug` on public types and add `Clone`, `Eq`, `Hash`, `Default`, or Serde traits only when
  their semantics are meaningful.
- Keep fields private when invariants matter; expose the smallest API required by callers.
- Use `pub(crate)` for workspace-internal implementation details and avoid unnecessary `pub` items;
  `coursers-core` enables `#![warn(unreachable_pub)]`.
- Document public contracts with `///`; use `//!` for module-level behavior and protocol invariants.
- Comments should explain non-obvious intent, safety, ordering, or compatibility constraints, not
  restate the code.
- Prefer iterator adapters and early returns/`let ... else` over index loops and deep nesting.
- Do not introduce `unsafe`; if unavoidable, document the invariant with a `// SAFETY:` comment.

## Error Handling

- Propagate recoverable library errors with `Result` and `?`; do not panic on external input.
- Use `CourserError` (`thiserror` + `miette::Diagnostic`) for reusable core error contracts.
- Use `miette::Result` at CLI/adapter boundaries when rich diagnostics are appropriate.
- Include actionable context in errors: path, event, rule, or failed operation.
- Hook commands generally fail open on malformed optional configuration: warn on stderr and preserve
  protocol-safe stdout. Preserve this behavior unless the protocol explicitly requires rejection.
- Never print diagnostics to stdout when stdout carries hook JSON.
- `unwrap`/`expect` are acceptable in tests for fixture setup and asserted invariants; avoid them in
  production paths unless impossibility is proven locally and explained.
- Do not silently discard errors. If best-effort behavior is required, emit a concise stderr warning.

## Testing Conventions

- Put focused unit tests in an adjacent `#[cfg(test)] mod tests` using `use super::*`.
- Put public behavior and binary protocol tests under `crates/*/tests/`.
- Use `tempfile` and injected ports to isolate HOME, config, state, and filesystem behavior.
- Cover success, malformed input, missing configuration, exceptions, and failure-learning thresholds.
- Assert exit status, stdout JSON shape, and stderr separately for subprocess tests.
- Keep serialized/protocol shapes backward compatible; use `#[serde(default)]` for additive fields.
- Add a regression test before fixing a reproduced rule, parser, hook, or rewrite defect.
- Environment-mutating tests must use the repository's lock pattern to avoid parallel test races.

## Hook and Configuration Safety

- Live block rules: `$HOME/.config/coursers/course-correct-rules.json` (`COURSERS_RULES` override).
- Failure state: project `.ctx/course-correct-state.json`, then global config fallback.
- Filters/rewrites: project `.ctx/crs-filters.toml`, then `$HOME/.config/crs/filters.toml`.
- Generic pipeline rules: `$HOME/.config/crs/plugins.d/*.toml`.
- Do not change user-global configuration unless explicitly requested.
- Reproduce hook bugs with the exact JSON payload; compare `crs probe`, `crs pre`, and
  `crs hook <event>` because their parsing/composition paths can differ.
- Inspect recent behavior with `crs log --limit N`; run `crs validate-hooks` after wiring changes.
- Treat plugin actions that execute commands as real side effects, not dry runs.
- Never expose secrets from payloads, logs, configuration, environment variables, or fixtures.

## Repository Instruction Files

No `.cursorrules`, `.cursor/rules/`, or `.github/copilot-instructions.md` files currently exist.
If one is added, treat its scoped instructions as additive to this file and resolve conflicts in
favor of the more specific rule.
