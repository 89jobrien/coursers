# coursers

Claude Code hook pipeline for course-correcting AI-generated Bash commands.

Two tools:

- **`coursers`** — PreToolUse/PostToolUse hooks that block bad commands and learn from failures
- **`crs`** — output filter and command rewriter _(in progress)_

---

## How It Works

### Rule-based blocking (`coursers pre`)

Reads the Claude Code `PreToolUse` JSON payload from stdin. If the `Bash` tool's command
matches a rule in `~/.claude/hooks/course-correct-rules.json`, the hook returns a `deny`
response with a human-readable message.

Rules support:

- Regex pattern matching (with optional `i` flag)
- Exception patterns — allow a command through even if the main pattern matches
- Per-rule messages

### Learned failure tracking (`coursers post`)

Reads the `PostToolUse` payload. When a `Bash` command exits non-zero (excluding signals and
intentional failures), it records the command in a rolling failure log. If the same command
fails ≥ N times in a sliding window, `coursers pre` will block it on the next attempt.

This catches commands that aren't covered by static rules but are clearly not working.

---

## Installation

```sh
cargo install --path crates/coursers
cargo install --path crates/crs
```

Wire into `~/.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      { "matcher": "Bash", "hooks": [{ "type": "command", "command": "coursers pre" }] }
    ],
    "PostToolUse": [
      { "matcher": "Bash", "hooks": [{ "type": "command", "command": "coursers post" }] }
    ]
  }
}
```

---

## Configuration

Rules file: `~/.claude/hooks/course-correct-rules.json`
(override with `COURSERS_RULES` env var)

```json
{
  "rules": [
    {
      "id": "no-grep-use-tool",
      "pattern": "\\bgrep\\b|\\brg\\b",
      "exceptions": ["\\| grep", "\\| rg"],
      "message": "Use the Grep tool instead of shell grep/rg."
    }
  ],
  "failure_learning": {
    "enabled": true,
    "block_threshold": 3,
    "window_seconds": 300,
    "cleanup_after_seconds": 3600,
    "max_tracked_commands": 200
  }
}
```

### Rule fields

| Field           | Required | Description                                              |
|-----------------|----------|----------------------------------------------------------|
| `id`            | yes      | Unique identifier shown in block messages                |
| `pattern`       | yes      | Regex matched against the full command string            |
| `pattern_flags` | no       | `"i"` for case-insensitive matching                      |
| `exceptions`    | no       | List of regexes — if any match, the rule is skipped      |
| `enabled`       | no       | Defaults to `true`                                       |
| `message`       | no       | Custom deny message; defaults to `Blocked by rule '<id>'`|

### Failure learning fields

| Field                    | Default | Description                                       |
|--------------------------|---------|---------------------------------------------------|
| `enabled`                | `true`  | Toggle the whole subsystem                        |
| `block_threshold`        | `3`     | Failures required to trigger a block              |
| `window_seconds`         | `300`   | Sliding window (5 minutes)                        |
| `cleanup_after_seconds`  | `3600`  | Remove entries not seen in this long              |
| `max_tracked_commands`   | `200`   | Evict oldest when over limit                      |
| `state_file`             | —       | Override state path; supports `~/` prefix         |
| `message_template`       | —       | Custom message with `{count}`, `{window}`, `{preview}` tokens |

---

## Workspace Structure

```
crates/
  core/        # shared library — rules, state, config
  coursers/    # `coursers` binary — pre/post hook handlers
  crs/         # `crs` binary — filter/rewrite (in progress)
agents/
  coursers-companion.md  # Claude Code agent for diagnostics
```

---

## crs (in progress)

`crs` will extend the pipeline with output compression and command rewriting:

- **`crs filter`** — PostToolUse hook; strips noise from command output (failures-only,
  errors-only, truncate modes) using rules in `.ctx/crs-filters.toml`
- **`crs rewrite`** — PreToolUse hook; rewrites commands to better forms (e.g. forces
  `--format json`) before they run
- **`crs discover`** — scans Claude Code session history to surface commands that match
  filter/rewrite rules but weren't intercepted

---

## License

MIT OR Apache-2.0
