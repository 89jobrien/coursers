---
name: coursers-maintain
description: Use whenever working on the Coursers project at /Users/joe/dev/coursers, developing or debugging its Rust workspace, changing Claude Code or Codex hook behavior, maintaining rules and filters, using or configuring the coursers/crs CLIs, investigating hook logs, or validating and installing local binaries. Also trigger for repo-specific questions about crate ownership, hook pipelines, command rewrites, output filtering, failure learning, replay, or project quality gates. For detailed hook-rule simulation, also load crs-hook-testing.
---

# Coursers Project Companion

Use this skill for both repository development and safe day-to-day operation of the
`coursers` and `crs` command-line tools.

## Orient First

1. Read `/Users/joe/dev/coursers/AGENTS.md` and
   `/Users/joe/dev/coursers/CLAUDE.md`; live project guidance overrides this skill.
2. Read `README.md` for user-facing CLI or configuration questions.
3. Check `git status --short --branch`. Preserve unrelated user-owned changes.
4. Check `.ctx/opavs/phase` and `.ctx/opavs/tasks.yaml`; use `opavs phase` and
   `opavs tasks` rather than manually changing workflow state.
5. Inspect the owning crate and nearby tests before editing.

## Workspace Map

Coursers is a Rust 2024 workspace with five crates:

- `crates/types` (`coursers-types`) — domain types and port contracts.
- `crates/core` (`coursers-core`) — rules, state, filters, rewrites, parsing, analysis,
  history, replay, and shared hook logic.
- `crates/coursers` (`coursers`) — CLI shell, concrete integration, and both binary
  targets: `coursers` and `crs`.
- `crates/e2e` (`coursers-e2e`) — end-to-end pipeline and scenario tests.
- `crates/xtask` (`xtask`) — taskit-backed workspace gates.

There is no `crates/crs` crate. The `coursers` package owns both binaries. Both parse the
same `Cli` and dispatch through `coursers::run`; inspect their small entrypoints before
assuming binary-specific behavior.

The workspace is incrementally moving contracts into `coursers-types`. Some compatibility
traits and re-exports still exist in `coursers-core`, so search current definitions and
consumers before moving or extending a port.

## Choose the Owner

- Domain data or stable port contract: `crates/types`.
- Business rules, parsing, matching, state transitions, filtering, rewriting, or analysis:
  `crates/core`.
- CLI flags, dispatch, stdin/stdout hook protocol, filesystem/process adapter, or command
  presentation: `crates/coursers`.
- Cross-command behavior and full hook scenarios: `crates/e2e`.
- CI gate selection or workspace automation: `crates/xtask` and `taskit.toml`.

Keep external I/O behind traits and inject fakes or tempdir-backed adapters in tests. Do
not mock the filesystem directly when an existing port can represent the dependency.

## Development Workflow

Use the narrowest useful gate first, then broaden:

```sh
cargo check --workspace
cargo clippy --workspace -- -D warnings
cargo nextest run -p coursers-types
cargo nextest run -p coursers-core
cargo nextest run -p coursers
cargo nextest run -p coursers-e2e
```

Repository shortcuts:

```sh
just check
just test
just e2e
just ci
nu scripts/smoke.nu
```

After changing Rust, run `cargo check`, clippy with warnings denied, and behavior-relevant
tests. Use `cargo test` only when `nextest` is unavailable or a project instruction calls
for it explicitly.

Builds and tests do not update the installed binaries. Before validating live hook
behavior, run:

```sh
just install
```

Then confirm that the invoked `crs`/`coursers` resolves to the newly installed binary.

## CLI and Configuration

The two binary names expose the shared command surface. Important operational groups are:

- Rule gate and learning: `pre`, `post`, `validate`, `probe`, `stats`, `history`.
- Output control: `filter`, `rewrite`.
- Analysis: `discover`, `insights`, `suggest`, `heat`, `replay`, `export`.
- Generic hook pipeline: `hook`, `validate-hooks`, `log`.
- Tool support: `audit`, `nu-check`.

The `coursers` entrypoint additionally intercepts `completions`; verify binary-specific
entrypoint behavior before presenting it as shared functionality.

Read `coursers --help` or `crs --help` before giving exact flags; the live Clap definition
in `crates/coursers/src/lib.rs` is authoritative.

Configuration precedence and paths are documented in `AGENTS.md` and `CLAUDE.md`. Common
files include:

- `$HOME/.config/coursers/course-correct-rules.json` — block rules.
- `.ctx/course-correct-state.json` — project-local failure-learning state.
- `$HOME/.config/coursers/course-correct-state.json` — global fallback state.
- `.ctx/crs-filters.toml` — project filter and rewrite rules.
- `$HOME/.config/crs/filters.toml` — global fallback filters and rewrites.
- `$HOME/.config/crs/plugins.d/*.toml` — generic hook pipeline rules.

Do not mutate user-global hook configuration unless the user explicitly requests it.
Validate configuration before trusting it in a live agent session.

## Hook Debugging

For a code-level hook defect:

1. Confirm environment, configured paths, and which installed binary is running.
2. Reproduce with the exact hook JSON payload and command string.
3. Compare `crs probe` with the real `crs pre` or `crs hook <event>` path; pipeline
   segmentation can make their results differ.
4. Inspect recent execution records with `crs log --limit N`.
5. Add the narrowest core or end-to-end regression test before changing behavior.

For adding, editing, or simulating a rule in
`$HOME/.config/crs/plugins.d/*.toml`, also load `crs-hook-testing`. That skill owns the
detailed simulation payloads and side-effect precautions; do not duplicate them here.

## Safety and Gotchas

- Treat hook simulations with `action = "run"` as real execution, not dry runs.
- Keep stdout protocol-clean for hook commands; diagnostics belong on stderr unless the
  protocol explicitly requires JSON output.
- Rule matching may inspect an entire command or split pipelines depending on the path.
- The `no-find-use-glob` rule can match words inside outer commands such as commit
  messages; test the exact payload rather than a quoted approximation.
- Distinguish `.ctx/GODMODE.trace.jsonl` from
  `.ctx/godmode/traces/trace.jsonl` when investigating trace tooling.
- `.ctx` is mostly ignored, so a clean `git status` does not prove local workflow state is
  unchanged.
- Never expose secrets from hook payloads, environment variables, or configuration files.

## Skill Handoffs

- Hook-rule configuration and simulation: `crs-hook-testing`.
- Unexpected behavior or failing tests: `godmode:systematic-debugging`.
- Rust API and style decisions: `godmode:rust-conventions` and `rust-api-guidelines`.
- Test strategy: `godmode:testing-philosophy`.
- CI failures: `godmode:ci-fix`.
- Release work: use the applicable release workflow skills rather than expanding this
  companion.
