# Coursers Companion

Agent context for working with the `coursers` + `crs` hook pipeline.

## Full hook chain

```
PreToolUse/Bash:
  1. coursers pre   — block commands matching course-correct rules or failure-learning threshold
  2. crs rewrite    — rewrite commands (e.g. force --message-format json); exit 1 = passthrough

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

- `crs filter` — PostToolUse; compresses/suppresses output per filter rules
- `crs rewrite` — PreToolUse; rewrites commands; exit 1 = passthrough unchanged
- `crs discover` — scans `~/.claude/projects/**/*.jsonl` for unhandled commands
- `crs validate` — rule health check
- `crs probe` — interactive rule matching (reads command from stdin)
- `crs stats` — cumulative block counts by rule

## Pipeline-aware matching

`coursers pre` splits commands on `&&`, `||`, `;` before rule evaluation. Each segment
is checked independently. Pipe (`|`) is NOT split — piped commands form one segment so
that exception patterns like `\| grep` match correctly.
