# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```sh
# Build
cargo build
cargo build --release

# Install binaries locally — one crate produces both the `coursers` and `crs` bins
just install   # or: cargo install --path crates/coursers

# Test
cargo test
cargo test -p coursers-types  # domain types and port traits only
cargo test -p coursers-core   # core library only
cargo test -p coursers        # coursers/crs binaries (both bin targets, one package)
cargo test -p coursers-e2e    # end-to-end pipeline/scenario tests

# Lint
cargo clippy --workspace -- -D warnings

# Smoke test (end-to-end)
nu scripts/smoke.nu

# Workspace task runner (taskit, package `xtask`) — wired as pre-commit/pre-push git hooks
cargo run -p xtask -- --help
```

## Architecture

Five-crate workspace:

```
crates/
  types/     # coursers-types — domain types and port traits
  core/      # coursers-core — shared library (rules, state, filters, rewrite, history)
  coursers/  # coursers package — TWO bin targets sharing one `src/main.rs`/CLI:
             #   `coursers` and `crs` are the same binary under two names, full command
             #   set on both (pre, post, filter, rewrite, discover, validate, probe,
             #   stats, insights, audit, suggest, history, export, hook, validate-hooks,
             #   log, heat, replay, nu-check)
  e2e/       # coursers-e2e — end-to-end pipeline/scenario integration tests
  xtask/     # taskit-based workspace CI runner (fmt/lint/test/coverage/pre-commit/pre-push/release)
```

There is no separate `crs` crate — `crs` is a second `[[bin]]` entry in
`crates/coursers/Cargo.toml`, both pointing at `src/main.rs`. Whichever name you invoke,
you get the identical CLI surface; the name only matters for which hook-chain command
convention you're following in `settings.json`.

### coursers-core

All domain logic lives here. Key modules:

- `rules` — loads `~/.config/coursers/course-correct-rules.json` (`COURSERS_RULES` env override);
  `RulesLoader` trait enables test injection
- `state` / `store` — rolling failure log; `StateStore` trait enables test injection
- `filters` — loads `.ctx/crs-filters.toml` (project) or `~/.config/crs/filters.toml` (global);
  four modes: `passthrough`, `failures-only`, `errors-only`, `truncate`
- `rewrite` — regex-replace rules from the same TOML file (`[[rewrites]]` sections)
- `history` — `CommandSource` trait + `discover()` function; scans Claude Code `.jsonl` session
  files to surface missed savings; uses `output_bytes / 4` for token estimates

### coursers / crs binary (one CLI, two bin names)

Run `coursers --help` or `crs --help` — both print the same 19-subcommand list. The hook
chain in `settings.json` conventionally calls `coursers` for the pre/post rule gate and
`crs` for everything else, but this is a naming convention, not an enforced split:

- `pre` — reads `PreToolUse` JSON from stdin; blocks if command matches a rule and no
  exception overrides; also blocks commands that have hit the failure threshold
- `post` — reads `PostToolUse` JSON from stdin; records non-zero exits to the
  failure-learning state file
- `filter` — PostToolUse hook; compresses/suppresses output per filter rules
- `rewrite` — PreToolUse hook; rewrites commands (e.g. force `--message-format json`);
  exit 1 = passthrough unchanged, exit 0 + JSON = rewritten
- `discover` — scans `~/.claude/projects/**/*.jsonl` for unhandled Bash commands
- `validate` — rule health check: pattern compiles, known triggers fire, exceptions work,
  alternative tools (bun, uv) on PATH
- `probe` — interactive: read command from stdin (raw string or JSON), show per-rule verdict
- `stats` — cumulative block counts by rule
- `insights` — session facets enriched with git context
- `audit` — rx prefix learning state
- `suggest` — suggest new rules from unhandled commands
- `history` — recent blocked commands
- `export` — dump rules + stats + state as portable JSON
- `hook` — run the generic hook pipeline for a hook event (also routes Codex hooks via
  `--target codex`, see `crates/coursers/src/crs_commands.rs`)
- `validate-hooks` — validate hook pipeline config
- `log` — query the hook execution log
- `heat` — heatmap of rule firings
- `replay` — replay a session's Bash commands through the current ruleset
- `nu-check` — validate nu scripts using `nu --ide-check`

### Hexagonal boundaries

`coursers-core` defines traits (`CommandSource`, `RulesLoader`, `StateStore`). The `coursers`
crate owns the concrete adapter (`JsonlCommandSource`). Tests inject fakes via the traits —
never mock the file system directly.

## Configuration files

| File                                           | Used by subcommand(s)                          | Purpose                                                 |
| ---------------------------------------------- | ----------------------------------------------- | ------------------------------------------------------- |
| `~/.config/coursers/course-correct-rules.json` | `pre`, `post`, `validate`, `probe`, `discover`  | Block rules + failure-learning config                   |
| `~/.config/coursers/course-correct-state.json` | `post`                                          | Global fallback failure-learning state                  |
| `.ctx/course-correct-state.json`               | `post`                                          | Project-local failure-learning state (wins over global) |
| `.ctx/crs-filters.toml`                        | `filter`, `rewrite`                            | Project-local filter and rewrite rules                  |
| `~/.config/crs/filters.toml`                   | `filter`, `rewrite`                            | Global fallback filter and rewrite rules                |

## Hook wiring (settings.json)

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          { "type": "command", "command": "coursers pre" },
          { "type": "command", "command": "crs rewrite" }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          { "type": "command", "command": "coursers post" },
          { "type": "command", "command": "crs filter" }
        ]
      }
    ]
  }
}
```

## Hook wiring verification

The standard hook chain is documented above and mirrored in
`agents/coursers-companion.md`. After installing the hook block in your local
`~/.claude/settings.json`, run `crs validate-hooks` to verify the chain.

## Council Analysis

```sh
op run --account=my.1password.com --env-file=/Users/joe/.secrets -- devkit council
# Run from repo root. No --repo flag. Output: ~/.dev-agents/coursers/ai-logs/
```

`devkit` is on PATH, but this command requires 1Password secrets injection and was
not re-run to verify the output path during the last doc pass — treat as
`[UNVERIFIED]` until confirmed.

## Coursers Rules Gotchas

- `no-find-use-glob` rule matches any command containing `\bfind\s+[./~$"']` —
  this includes git commit messages with phrases like "find .ctx". Exception added
  for `git (commit|log|tag|stash)` including `git -C` form.
- Two godmode trace-log files exist and are easy to confuse: `.ctx/GODMODE.trace.jsonl`
  (repo root) vs `.ctx/godmode/traces/trace.jsonl` (the one
  `skills/observability-as-infrastructure/helpers/*.nu` actually read). Check which
  one before diagnosing "broken" trace tooling.
- `.ctx/*` is gitignored except explicitly allow-listed files — edits to
  `.ctx/GODMODE.tasks.yaml` are invisible to `git status` and `godmode handoff`'s
  uncommitted-file warning. Don't assume a clean handoff report means no local state
  changed there.
- `cargo deny check advisories` crashing with "unsupported CVSS version: 4.0" is a
  `cargo-deny` binary bug (fixed in 0.20.2, broken in 0.18.3), not a `deny.toml`
  config problem — ignoring the advisory ID doesn't help since the crash happens
  loading the advisory DB, before ignore-filtering applies. Upgrade the binary
  (mise-managed: `mise use -g cargo:cargo-deny@latest`, then start a fresh shell —
  an already-running shell's `PATH` has the old version's dir baked in).
- After editing `crates/core` or `crates/coursers`, `cargo build`/`cargo nextest run`
  do NOT update the globally-installed `~/.cargo/bin/crs` and `coursers` binaries.
  Run `just install` (or `cargo install --path crates/coursers`) before trusting
  live `crs`/`coursers` behavior against the new code — this cost two separate
  debugging detours on 2026-08-15/16 (rewrite-engine cascade fix, hook-log fix)
  and a third on 2026-08-23 (miette diagnostic rendering) before the pattern was
  recognized.
- When a rule seems to behave differently through the real hook chain than
  expected, compare `crs probe` (matches the raw whole command string, no
  pipeline segmentation) against `crs pre` fed the identical JSON payload
  (goes through the real hook path, including `check_pipeline`'s segment-
  splitting on `;`/`&&`/`||`). This divergence isolated the `no-bash-use-nu`
  pipeline-splitting bug (doob todo `b6ff3600-0733-41fd-95c4-cfc5bf03b385`)
  in one step.

## Godmode Skills

Godmode lives at `~/dev/godmode` — a library of reusable skills and agents available in any
Claude Code session. Relevant skills for coursers development:

| Skill                             | When to use                                               |
| --------------------------------- | --------------------------------------------------------- |
| `godmode:ci-fix`                  | CI failing — self-healing diagnosis + fix loop            |
| `godmode:systematic-debugging`    | Any test failure, panic, or unexpected behavior           |
| `godmode:code-review`             | Before merging — structured review of implementation      |
| `godmode:cap`                     | Commit + push with pre-flight validation                  |
| `godmode:task-driven-development` | Before writing impl — TDD scaffold + task graph           |
| `godmode:testing-philosophy`      | Designing a test strategy for new modules                 |
| `godmode:refactoring`             | Restructuring code without changing behavior              |
| `godmode:health-score`            | Measure codebase health (tests, clippy, TODOs, coverage)  |
| `godmode:dead-code`               | Find unused public API surface and orphaned modules       |
| `godmode:pr-author`               | Compose PR descriptions from branch diff + commit history |

Invoke via `/godmode:<skill-name>` in the Claude Code prompt, or use the `Skill` tool directly.
Agents live at `~/dev/godmode/agents/`. Some are domain-prefixed (`dbg__`, `git__`, `agent__`,
etc.); many others (`forge.md`, `conductor.md`, `envoy.md`, ...) have no prefix at all —
check `~/dev/godmode/agents/INDEX.md` rather than assuming a naming pattern.

## HANDOFF Dependency Fields

Use structured fields, not free-text notes, for dependency tracking:

- `blocked_by: [id1, id2]` on the blocked item
- `unblocks: [id1]` on each blocker
