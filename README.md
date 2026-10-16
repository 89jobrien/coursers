# coursers

Claude Code hook pipeline for course-correcting AI-generated Bash commands.

Two tools:

- **`coursers`** — PreToolUse/PostToolUse hooks that block bad commands and learn from failures
- **`crs`** — front controller, output filter, command rewriter, and hook validator

---

## Quick Start

```bash
# Install both binaries
just install

# Check the rules compile and the examples fire as expected
coursers validate

# See what a rule would do to a command before running it
echo 'git commit -am "wip"' | crs probe

# Rewrite a command into its prefixed equivalent, or exit 1 if blocked
crs rewrite

# Review what the hooks have blocked and learned
crs log --limit 20
```

`coursers` and `crs` are wired into `~/.claude/settings.json` as `PreToolUse`
and `PostToolUse` hooks. Once installed, a matching command is denied before
it runs, and a command that keeps failing is denied on its Nth attempt even
with no rule written for it.

---

## Commands

All 20 subcommands, in the order `coursers --help` lists them. `crs` exposes
the same model. The sections below go deeper on the ones that carry the
design.

| Command | Effect |
| --- | --- |
| `pre` | PreToolUse hook — reads JSON payload from stdin, writes hook response to stdout |
| `post` | PostToolUse hook — reads JSON payload from stdin, records failures |
| `filter` | Filter PostToolUse output — reads hook JSON from stdin, emits hook response to stdout |
| `rewrite` | Rewrite a PreToolUse command — reads hook JSON from stdin, emits rewritten command or exits 1 |
| `discover` | Discover missed savings from Claude Code session history |
| `validate` | Validate rules: check patterns compile, examples fire, exceptions work |
| `probe` | Probe a command against all rules — reads command from stdin |
| `stats` | Show cumulative block counts by rule |
| `insights` | Analyze session facets enriched with git context |
| `audit` | Show rx prefix learning state |
| `suggest` | Suggest new rules from unhandled commands |
| `history` | Show recent blocked commands |
| `export` | Dump rules + stats + state as portable JSON |
| `hook` | Run the generic hook pipeline for a hook event |
| `validate-hooks` | Validate hook pipeline config |
| `log` | Query the hook execution log |
| `heat` | Show heatmap of rule firings |
| `replay` | Replay a session's Bash commands through the current ruleset |
| `nu-check` | Validate nu scripts using `nu --ide-check` |
| `help` | Print this message or the help of the given subcommand(s) |

---

## How It Works

### Rule-based blocking (`coursers pre`)

Reads the Claude Code `PreToolUse` JSON payload from stdin. If the `Bash` tool's command
matches a rule in `~/.config/coursers/course-correct-rules.json`, the hook returns a `deny`
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
just install
```

That is a wrapper for `cargo install --path crates/coursers`, which installs
**both** binaries from the one package: `coursers` and `crs`. Installing by
path skips the release profile and the two-binary wiring, so prefer `just
install` unless you are testing a local build.

Install the front-controller hook chain into your local `~/.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          { "type": "command", "command": "crs hook pre-tool-use" }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          { "type": "command", "command": "crs hook post-tool-use" }
        ]
      }
    ]
  }
}
```

Run `crs validate-hooks` after installing the hook block to verify the chain is wired
correctly.

For Codex, the top-level registry also points at `crs hook --target codex <event>`.
Those entries dispatch to the existing `$HOME/.codex/hooks/*.crux` backends, which
still compose `coursers pre/post` and `crs rewrite/filter` internally for Bash flows.

For OpenCode, copy or symlink exactly `integrations/opencode/coursers.ts` and
`integrations/opencode/opencode-plugin.d.ts` into project-local `.opencode/plugins/` or global
`$HOME/.config/opencode/plugins/`. Do not copy the integration test file. Run
`crs validate-hooks --target opencode` to verify both files and the required binaries. See
[docs/opencode.md](docs/opencode.md) for event mappings and troubleshooting.

---

## Configuration

Rules file: `~/.config/coursers/course-correct-rules.json`
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

| Field           | Required | Description                                               |
| --------------- | -------- | --------------------------------------------------------- |
| `id`            | yes      | Unique identifier shown in block messages                 |
| `pattern`       | yes      | Regex matched against the full command string             |
| `pattern_flags` | no       | `"i"` for case-insensitive matching                       |
| `exceptions`    | no       | List of regexes — if any match, the rule is skipped       |
| `enabled`       | no       | Defaults to `true`                                        |
| `message`       | no       | Custom deny message; defaults to `Blocked by rule '<id>'` |

### Failure learning fields

| Field                   | Default | Description                                                   |
| ----------------------- | ------- | ------------------------------------------------------------- |
| `enabled`               | `true`  | Toggle the whole subsystem                                    |
| `block_threshold`       | `3`     | Failures required to trigger a block                          |
| `window_seconds`        | `300`   | Sliding window (5 minutes)                                    |
| `cleanup_after_seconds` | `3600`  | Remove entries not seen in this long                          |
| `max_tracked_commands`  | `200`   | Evict oldest when over limit                                  |
| `state_file`            | —       | Override state path; supports `~/` prefix                     |
| `message_template`      | —       | Custom message with `{count}`, `{window}`, `{preview}` tokens |

---

## crs

`crs` extends the pipeline with output compression and command rewriting.

### `crs filter` (PostToolUse)

Reads PostToolUse output and suppresses or compresses it based on rules in
`.ctx/crs-filters.toml` (project-local) or `~/.config/crs/filters.toml` (global).

Filter modes per rule:

| Mode            | Behaviour                                             |
| --------------- | ----------------------------------------------------- |
| `passthrough`   | Output unchanged (default)                            |
| `failures-only` | Suppress output on exit 0; pass through on failure    |
| `errors-only`   | Only pass lines containing "error" (case-insensitive) |
| `truncate`      | Keep first N lines (`max_lines`, default 50)          |

Example `.ctx/crs-filters.toml`:

```toml
[[filters]]
pattern = "cargo (build|check|clippy)"
mode = "errors-only"

[[filters]]
pattern = "cargo nextest"
mode = "failures-only"
```

Wire into your local `~/.claude/settings.json`:

```json
{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "crs filter" }] }
```

### `crs rewrite` (PreToolUse)

Rewrites commands before they run using regex replace rules. Exit 0 + JSON response means
rewritten; exit 1 means passthrough unchanged.

```toml
[[rewrites]]
pattern = "^cargo build$"
replace = "cargo build --message-format json"
```

Wire into your local `~/.claude/settings.json`:

```json
{
  "matcher": "Bash",
  "hooks": [{ "type": "command", "command": "crs rewrite" }]
}
```

### `crs discover`

Scans Claude Code session history (`~/.claude/projects/**/*.jsonl`) to surface Bash
commands that match filter/rewrite rules but weren't intercepted.

---

## Workspace Structure

Five workspace members, plus a standalone fuzz workspace:

```text
crates/
  types/     # coursers-types — domain data and stable port traits
  core/      # coursers-core — rules, parsing, state, filters, rewrite,
             #   analysis, and hook composition
  coursers/  # coursers — CLI dispatch, concrete adapters, protocol I/O.
             #   Defines both binaries: `coursers` from src/main.rs and
             #   `crs` from src/bin/crs.rs, both calling coursers::run
  e2e/       # coursers-e2e — cross-command and hook-chain scenarios
  xtask/     # taskit-backed workspace quality gates
fuzz/        # standalone cargo-fuzz workspace, not a member
schemas/     # rule schema definitions
docs/        # mdbook sources
agents/      # coursers-companion.md — Claude Code agent for diagnostics
config/      # pipeline rule plugins
scripts/     # smoke.nu — end-to-end smoke test
tests/       # binary integration test fixtures
```

---

## License

MIT OR Apache-2.0
