# Coursers Companion

Agent context for working with the `coursers` + `crs` hook pipeline.

## Full hook chain

```
PreToolUse/Bash:
  1. coursers pre   — block commands matching course-correct rules or failure-learning threshold
  2. crs rewrite    — rewrite commands; exit 1 = passthrough

PostToolUse/Bash:
  1. coursers post  — record non-zero exits to failure-learning state
  2. crs filter     — compress/suppress output per filter rules
```

Wired in `~/.claude/settings.json`:

```json
"PreToolUse": [
  {
    "matcher": "Bash",
    "hooks": [
      { "type": "command", "command": "/Users/joe/.cargo/bin/coursers pre" },
      { "type": "command", "command": "/Users/joe/.cargo/bin/crs rewrite" }
    ]
  }
],
"PostToolUse": [
  {
    "matcher": "Bash",
    "hooks": [
      { "type": "command", "command": "/Users/joe/.cargo/bin/coursers post" },
      { "type": "command", "command": "/Users/joe/.cargo/bin/crs filter" }
    ]
  }
]
```

After installing the hook block in your local settings file, run
`crs validate-hooks` to verify the chain.

## Binaries

| Binary     | Crate             | Install                                |
| ---------- | ----------------- | -------------------------------------- |
| `coursers` | `crates/coursers` | `cargo install --path crates/coursers` |
| `crs`      | `crates/crs`      | `cargo install --path crates/crs`      |

## Config files

| File                                           | Purpose                                |
| ---------------------------------------------- | -------------------------------------- |
| `~/.config/coursers/course-correct-rules.json` | Block rules + failure-learning config  |
| `~/.config/coursers/course-correct-state.json` | Global failure-learning state          |
| `.ctx/course-correct-state.json`               | Project-local state (overrides global) |
| `.ctx/crs-filters.toml`                        | Project-local filter + rewrite rules   |
| `~/.config/crs/filters.toml`                   | Global fallback filter + rewrite rules |

## Subcommands

### coursers

- `coursers pre` — PreToolUse; blocks commands matching rules or failure threshold
- `coursers post` — PostToolUse; records non-zero exits to failure-learning state

### crs

- `crs filter` — PostToolUse; compresses/suppresses output per filter rules
- `crs rewrite` — PreToolUse; rewrites commands; exit 1 = passthrough unchanged
- `crs discover` — scans `~/.claude/projects/**/*.jsonl` for unhandled commands
- `crs validate` — rule health check: patterns compile, examples fire, alternatives on PATH
- `crs probe` — interactive rule matching (reads command from stdin)
- `crs stats` — cumulative block counts by rule
- `crs insights` — session facets enriched with git context (text/json, --since, --repo)
- `crs audit` — show rx prefix learning state; confirmed mappings and pending probes
- `crs suggest` — suggest new rules from top unhandled commands in session history
- `crs history` — show recent blocked commands with timestamps and firing rules
- `crs export` — dump rules + stats + state as a portable JSON snapshot
- `crs heat` — heatmap of rule firings by hour-of-day and day-of-week
- `crs replay` — replay a session's Bash commands through the current ruleset (dry-run)

## crs-core modules

Organized into three module groups:

### `parse/` — command parsing

- `ast` — shell command AST representation
- `expand` — variable and tilde expansion
- `pipeline` — pipeline splitting (&&, ||, ;) for per-segment rule evaluation

### `hook/` — hook runtime

- `filters` — output filter modes: passthrough, failures-only, errors-only, truncate
- `rewrite` — regex-replace rules from TOML `[[rewrites]]` sections
- `tool_swap` — tool alternative detection (e.g. bun for npm, uv for pip)

### `analyze/` — session analysis

- `capture` — command capture from hook payloads
- `heat` — heatmap data aggregation
- `history` — `CommandSource` trait + `discover()` for session scanning
- `insights` — session facet analysis with git context
- `stats` — cumulative rule-firing statistics
- `suggest` — rule suggestion engine from unhandled command patterns

### Top-level modules

- `config` — configuration loading and resolution
- `date` — date math utilities (Rata Die conversions)
- `loader` — `RulesLoader` trait and implementations
- `obfsck` — obfsck filter generation from discover results
- `replay` — session replay engine (dry-run rule evaluation)
- `rtk` — RTK token-saving integration
- `rules` — rule definitions and matching logic
- `rx_prefix` — rx prefix learning state management
- `state` / `store` — rolling failure log; `StateStore` trait for test injection
- `testing` — `MockWorkspace` builder for test fixtures (behind `testing` feature)

## Pipeline-aware matching

`coursers pre` splits commands on `&&`, `||`, `;` before rule evaluation. Each segment
is checked independently. Pipe (`|`) is NOT split — piped commands form one segment so
that exception patterns like `\| grep` match correctly.

## Hexagonal boundaries

`crs-core` defines traits (`CommandSource`, `RulesLoader`, `StateStore`). The `crs` binary
owns concrete adapters (`JsonlCommandSource`). Tests inject fakes via the traits — never
mock the file system directly.
