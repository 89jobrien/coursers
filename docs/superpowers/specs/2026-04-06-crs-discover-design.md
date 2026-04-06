# crs discover — Design Spec

Date: 2026-04-06
Status: approved

## Purpose

`crs discover` mines Claude Code session history to produce a savings report: which Bash
commands were intercepted by existing `crs filter` rules, and which were not. Modeled on
`rtk discover`. Helps identify which new rules to write from real usage data rather than guessing.

## CLI

```
crs discover [OPTIONS]

Options:
  -a, --all         Scan all projects (default: current project only)
  -l, --limit N     Max rows per section [default: 15]
  -s, --since N     Limit to sessions from last N days [default: 30]
  -f, --format FMT  Output format: text | json [default: text]
```

Source root: `CLAUDE_PROJECTS_DIR` env var → `~/.claude/projects` fallback.
Current project detection: match session `cwd` against `std::env::current_dir()`.

## Architecture

Hexagonal — domain is pure, fs access isolated to an adapter.

```
crs-core/src/history.rs          — port + domain logic
  trait CommandSource             — yields (command: String, session_id: String, cwd: String)
  fn discover(src, rules, opts) -> DiscoverReport
  struct DiscoverReport { intercepted, unhandled, scanned_sessions, scanned_commands }
  struct CommandFreq { stem, count, example, est_tokens }

crates/crs/src/jsonl_source.rs   — infrastructure adapter
  struct JsonlCommandSource       — walks *.jsonl, extracts Bash tool_use
```

`crs-core` gains no new dependencies. `crs` binary gains `walkdir` + `serde_json` (already
present).

## Domain: `CommandSource` trait

```rust
pub trait CommandSource {
    fn commands(&self) -> impl Iterator<Item = CommandRecord>;
}

pub struct CommandRecord {
    pub command: String,
    pub session_id: String,
    pub cwd: String,
    pub timestamp: Option<String>,
}
```

## Domain: `discover()`

```rust
pub struct DiscoverOpts {
    pub limit: usize,
    pub since_days: Option<u32>,
    pub all_projects: bool,
    pub current_dir: Option<PathBuf>,
}

pub fn discover(
    source: &impl CommandSource,
    rules: &[Rule],
    opts: &DiscoverOpts,
) -> DiscoverReport
```

Steps:

1. Filter records by `since_days` (compare timestamp prefix) and project cwd if `!all_projects`.
2. For each command: compute stem (first 1–2 whitespace-delimited tokens, strip leading env
   assignments and path prefixes).
3. Check stem against each rule pattern — first match wins.
4. Accumulate into `intercepted` (matched) and `unhandled` (no match) frequency maps, keeping
   one example command per stem.
5. Estimate tokens: `count * 150` (conservative flat rate per intercept).
6. Sort both maps by count descending, truncate to `opts.limit`.

## Infrastructure: `JsonlCommandSource`

Walks `<root>/<project>/*.jsonl` (or all projects with `--all`). For each line:

- Parse as JSON.
- Skip unless `type == "assistant"`.
- Iterate `message.content[]` for `{ type: "tool_use", name: "Bash" }` blocks.
- Yield `CommandRecord { command: input.command, session_id, cwd, timestamp }`.

Skips malformed lines silently. Streams line-by-line — no full-file load.

## Output: text format

```
CRS Discover — Savings Opportunities
====================================================
Scanned: N sessions, M Bash commands

INTERCEPTED — commands with matching rules
------------------------------------------------------------------------
Command              Count   Rule                 Est. Savings
cargo nextest           32   no-cargo-nextest     ~4.8K tokens
doob todo               18   no-doob-direct       ~2.7K tokens
------------------------------------------------------------------------
Total: 50 commands → ~7.5K tokens saveable

TOP UNHANDLED — no matching rule
----------------------------------------------------
Command              Count   Example
op account               9   op account list 2>&1
python3                  2   python3 /tmp/validate...
----------------------------------------------------
~estimated at 150 tokens/intercept
```

## Output: json format

```json
{
  "scanned_sessions": 15,
  "scanned_commands": 199,
  "intercepted": [
    {
      "stem": "cargo nextest",
      "count": 32,
      "example": "...",
      "est_tokens": 4800
    }
  ],
  "unhandled": [
    { "stem": "op account", "count": 9, "example": "op account list 2>&1" }
  ]
}
```

## Stem Extraction Rules

1. Strip leading env assignments (`KEY=val`).
2. Strip known path prefixes (drop everything before the last `/` on token 0 if it contains `/`).
3. Take token 0 as base. If token 1 is a subcommand (no leading `-`), append it: `cargo nextest`,
   `doob todo`, `git log`. Otherwise stem = token 0 only.

## Testing

- Unit tests in `crs-core/src/history.rs` using a `Vec<CommandRecord>` as `CommandSource`.
- Cover: stem extraction edge cases, rule matching, since_days filter, project filter, limit.
- Integration test in `crs` binary crate using fixture `.jsonl` files (reuse existing fixture
  dir).
