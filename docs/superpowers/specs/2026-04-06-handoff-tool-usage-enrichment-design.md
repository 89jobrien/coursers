# Design: Handoff Tool Usage Enrichment

**Date:** 2026-04-06
**Status:** Approved; amended 2026-09-05 to use one canonical implementation

## Problem

Handoff files (`HANDOFF.yaml`, `.ctx/HANDOFF.state.yaml`) carry task and build state but no
information about tool usage patterns from the session. The next Claude session has no visibility
into which commands dominated, what savings were left on the table, or which patterns aren't
covered by rules.

## Solution

The canonical `scripts/enrich-handoff.nu` script runs at end of session, pulls RTK session data via `rtk discover --format json`, and writes two outputs. The deprecated `scripts/enrich-handoff.sh` path remains as a compatibility wrapper that delegates to Nushell:

1. `.ctx/HANDOFF.tools.yaml` — full detail for `/hand:on` orientation and `/hand:over` diagrams
2. A `tool_usage` stats block merged into `.ctx/HANDOFF.state.yaml`

All transformation logic lives in the Nushell script. Callers invoke it directly. `rtk` absent causes a non-zero exit without writing output.

## Data Flow

```
rtk discover --format json --since <N>
        ↓
scripts/enrich-handoff.nu
        ├─→ .ctx/HANDOFF.tools.yaml        (full detail)
        └─→ .ctx/HANDOFF.state.yaml        (tool_usage stats block merged in)
```

## Output Schemas

### `.ctx/HANDOFF.tools.yaml`

```yaml
generated: 2026-04-06
since_days: 1
sessions_scanned: 34
total_commands: 414
top_supported:
  - command: cargo test
    count: 69
    rtk_equivalent: rtk cargo
    est_savings_tokens: 12894
    est_savings_pct: 80
top_unhandled:
  - base_command: op account
    count: 19
    example: "op account list 2>&1"
```

### `tool_usage` block in `.ctx/HANDOFF.state.yaml`

```yaml
tool_usage:
  sessions_scanned: 34
  total_commands: 414
  top_command: "cargo test (69)"
  est_savings_tokens: 19217
  unhandled_top: "op account (19)"
```

## Script Interface

```
scripts/enrich-handoff.nu [--since N] [--input PATH] [--root PATH] [--generated-date DATE]
```

- `--since N` — scan last N days (default 1, covers current session)
- `--input PATH` — read saved RTK JSON instead of running RTK (for deterministic testing)
- `--root PATH` — write into a supplied repository root instead of running `handoff-detect`
- `--generated-date DATE` — override the generated date for deterministic fixtures
- The script writes to `.ctx/` relative to the repo root (resolved via `handoff-detect --root`)
- If `rtk` is not on PATH: exit non-zero, write nothing
- If `.ctx/` does not exist: create it

## Integration with `hand` Skills

### `hand:off`

After writing `.ctx/HANDOFF.state.yaml`, runs:

```sh
nu scripts/enrich-handoff.nu 2>/dev/null || true
```

### `hand:on`

After reading `.ctx/HANDOFF.state.yaml`, if `tool_usage` is present, surfaces a one-line summary
before P0 triage:

```
Tool usage (last session): 414 commands · ~19K tokens saveable · top unhandled: op account (19)
```

If `.ctx/HANDOFF.tools.yaml` exists, notes it's available for deeper inspection.

### `hand:over`

If `.ctx/HANDOFF.tools.yaml` exists, adds a **Tool Usage** section to `.ctx/HANDOVER.md`
containing:

- Summary table (sessions, commands, estimated savings)
- Mermaid `xychart-beta` bar chart of top commands by count

## Files Touched

| File | Change |
|------|--------|
| `scripts/enrich-handoff.nu` | New — primary script |
| `scripts/enrich-handoff.sh` | Deprecated compatibility wrapper delegating to the canonical Nushell script |
| `hand` plugin: `handoff` skill | Call enrich script after state write |
| `hand` plugin: `handon` skill | Surface `tool_usage` summary on wake |
| `hand` plugin: `handover` skill | Add Tool Usage section + Mermaid chart |

## Non-Goals

- No changes to `HANDOFF.yaml` (committed file) — tool usage is ephemeral session data
- No new Rust code in this repo at this stage
- `crs discover` integration deferred — RTK is the data source for now
