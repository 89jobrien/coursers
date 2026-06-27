# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```sh
# Build
cargo build
cargo build --release

# Install binaries locally
cargo install --path crates/coursers
cargo install --path crates/crs

# Test
cargo test
cargo test -p crs-core        # core library only
cargo test -p coursers        # pre/post hook binary
cargo test -p crs             # filter/rewrite/discover binary

# Lint
cargo clippy --workspace -- -D warnings

# Smoke test (end-to-end)
nu scripts/smoke.nu
```

## Architecture

Three-crate workspace:

```
crates/
  core/      # crs-core — shared library (rules, state, filters, rewrite, history)
  coursers/  # `coursers` binary — pre/post hook handlers
  crs/       # `crs` binary — filter, rewrite, discover, validate, probe
```

### crs-core

All domain logic lives here. Key modules:

- `rules` — loads `~/.config/coursers/course-correct-rules.json` (`COURSERS_RULES` env override);
  `RulesLoader` trait enables test injection
- `state` / `store` — rolling failure log; `StateStore` trait enables test injection
- `filters` — loads `.ctx/crs-filters.toml` (project) or `~/.config/crs/filters.toml` (global);
  four modes: `passthrough`, `failures-only`, `errors-only`, `truncate`
- `rewrite` — regex-replace rules from the same TOML file (`[[rewrites]]` sections)
- `history` — `CommandSource` trait + `discover()` function; scans Claude Code `.jsonl` session
  files to surface missed savings; uses `output_bytes / 4` for token estimates

### coursers binary

Two subcommands wired as Claude Code hooks:

- `coursers pre` — reads `PreToolUse` JSON from stdin; blocks if command matches a rule and no
  exception overrides; also blocks commands that have hit the failure threshold
- `coursers post` — reads `PostToolUse` JSON from stdin; records non-zero exits to the
  failure-learning state file

### crs binary

Five subcommands:

- `crs filter` — PostToolUse hook; compresses/suppresses output per filter rules
- `crs rewrite` — PreToolUse hook; rewrites commands (e.g. force `--message-format json`);
  exit 1 = passthrough unchanged, exit 0 + JSON = rewritten
- `crs discover` — scans `~/.claude/projects/**/*.jsonl` for unhandled Bash commands
- `crs validate` — rule health check: pattern compiles, known triggers fire, exceptions work,
  alternative tools (bun, uv) on PATH
- `crs probe` — interactive: read command from stdin (raw string or JSON), show per-rule verdict

### Hexagonal boundaries

`crs-core` defines traits (`CommandSource`, `RulesLoader`, `StateStore`). The `crs` binary owns
the concrete adapter (`JsonlCommandSource`). Tests inject fakes via the traits — never mock the
file system directly.

## Configuration files

| File                                           | Used by                                            | Purpose                                                 |
| ---------------------------------------------- | -------------------------------------------------- | ------------------------------------------------------- |
| `~/.config/coursers/course-correct-rules.json` | `coursers pre/post`, `crs validate/probe/discover` | Block rules + failure-learning config                   |
| `~/.config/coursers/course-correct-state.json` | `coursers post`                                    | Global fallback failure-learning state                  |
| `.ctx/course-correct-state.json`               | `coursers post`                                    | Project-local failure-learning state (wins over global) |
| `.ctx/crs-filters.toml`                        | `crs filter/rewrite`                               | Project-local filter and rewrite rules                  |
| `~/.config/crs/filters.toml`                   | `crs filter/rewrite`                               | Global fallback filter and rewrite rules                |

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

## Hook wiring

The standard hook chain is documented above and mirrored in
`agents/coursers-companion.md`. After installing the hook block in your local
`~/.claude/settings.json`, run `crs validate-hooks` to verify the chain.

## Council Analysis

```sh
op run --account=my.1password.com --env-file=/Users/joe/.secrets -- devkit council
# Run from repo root. No --repo flag. Output: ~/.dev-agents/coursers/ai-logs/
```

## Coursers Rules Gotchas

- `no-find-use-glob` rule matches any command containing `\bfind\s+[./~$"']` —
  this includes git commit messages with phrases like "find .ctx". Exception added
  for `git (commit|log|tag|stash)` including `git -C` form.

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
Agents live at `~/dev/godmode/agents/` — prefixed by domain (`dbg__`, `qual__`, `plan__`, etc.).

## HANDOFF Dependency Fields

Use structured fields, not free-text notes, for dependency tracking:

- `blocked_by: [id1, id2]` on the blocked item
- `unblocks: [id1]` on each blocker
