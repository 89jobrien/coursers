# Handoff Tool Usage Enrichment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enrich handoff files with RTK session tool usage data — full detail in
`.ctx/HANDOFF.tools.yaml`, summary stats in `.ctx/HANDOFF.state.yaml` — surfaced by `hand:on`
and `hand:over`.

**Architecture:** A Nushell script (`scripts/enrich-handoff.nu`) and POSIX sh fallback
(`scripts/enrich-handoff.sh`) call `rtk discover --format json`, shape the output, and write
two `.ctx/` files. The `hand` plugin skills are updated to call the script, read the outputs,
and surface the data.

**Tech Stack:** Nushell (`nu`), POSIX sh, `rtk discover --format json`, YAML (written as plain
text — no parser dependency), `handoff-detect --root`

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `scripts/enrich-handoff.nu` | Create | Primary script — nu implementation |
| `scripts/enrich-handoff.sh` | Create | POSIX fallback — identical output schema |
| `~/.claude/plugins/hand/hand/skills/handoff/skill.md` | Modify | Call enrich script after state write (step 5) |
| `~/.claude/plugins/hand/hand/skills/handon/skill.md` | Modify | Surface `tool_usage` summary after state read |
| `~/.claude/plugins/hand/hand/skills/handover/skill.md` | Modify | Add Tool Usage section + Mermaid chart |

---

## Task 1: Write `scripts/enrich-handoff.nu`

**Files:**
- Create: `scripts/enrich-handoff.nu`

- [ ] **Step 1: Validate nu syntax stub compiles**

Write the file:

```nu
#!/usr/bin/env nu
# enrich-handoff.nu — write .ctx/HANDOFF.tools.yaml and update .ctx/HANDOFF.state.yaml
# Usage: nu scripts/enrich-handoff.nu [--since <int>]

def main [--since: int = 1] {
    # Verify rtk is on PATH
    let rtk_path = (which rtk | get path | first? | default "")
    if $rtk_path == "" {
        exit 0
    }

    # Resolve repo root via handoff-detect
    let root_result = (do { handoff-detect --root } | complete)
    if $root_result.exit_code != 0 {
        exit 0
    }
    let root = ($root_result.stdout | str trim)
    let ctx = ($root | path join ".ctx")

    # Create .ctx/ if absent
    mkdir $ctx

    # Run rtk discover
    let discover_result = (do { rtk discover --format json --since $since } | complete)
    if $discover_result.exit_code != 0 {
        exit 0
    }
    let data = ($discover_result.stdout | from json)

    write_tools_yaml $ctx $data $since
    merge_state_yaml $ctx $data
}

def write_tools_yaml [ctx: string, data: record, since: int] {
    let today = (date now | format date "%Y-%m-%d")

    let top_supported = (
        $data.supported?
        | default []
        | first 10
        | each {|r| {
            command: $r.command,
            count: $r.count,
            rtk_equivalent: $r.rtk_equivalent,
            est_savings_tokens: $r.estimated_savings_tokens,
            est_savings_pct: $r.estimated_savings_pct
        }}
    )

    let top_unhandled = (
        $data.unsupported?
        | default []
        | first 10
        | each {|r| {
            base_command: $r.base_command,
            count: $r.count,
            example: $r.example
        }}
    )

    let lines = [
        $"generated: ($today)"
        $"since_days: ($since)"
        $"sessions_scanned: ($data.sessions_scanned)"
        $"total_commands: ($data.total_commands)"
        "top_supported:"
    ] ++ (
        $top_supported | each {|r|
            [
                $"  - command: ($r.command)"
                $"    count: ($r.count)"
                $"    rtk_equivalent: ($r.rtk_equivalent)"
                $"    est_savings_tokens: ($r.est_savings_tokens)"
                $"    est_savings_pct: ($r.est_savings_pct)"
            ]
        } | flatten
    ) ++ ["top_unhandled:"] ++ (
        $top_unhandled | each {|r|
            [
                $"  - base_command: ($r.base_command)"
                $"    count: ($r.count)"
                $"    example: \"($r.example | str replace '"' '\"')\""
            ]
        } | flatten
    )

    $lines | str join "\n" | save --force ($ctx | path join "HANDOFF.tools.yaml")
}

def merge_state_yaml [ctx: string, data: record] {
    let state_path = ($ctx | path join "HANDOFF.state.yaml")

    # Compute summary values
    let top_cmd = (
        $data.supported? | default [] | first? | default null
        | if $in != null { $"($in.command) (($in.count))" } else { "" }
    )
    let total_savings = (
        $data.supported? | default []
        | get estimated_savings_tokens
        | math sum
    )
    let top_unhandled = (
        $data.unsupported? | default [] | first? | default null
        | if $in != null { $"($in.base_command) (($in.count))" } else { "" }
    )

    let block = [
        "tool_usage:"
        $"  sessions_scanned: ($data.sessions_scanned)"
        $"  total_commands: ($data.total_commands)"
        $"  top_command: \"($top_cmd)\""
        $"  est_savings_tokens: ($total_savings)"
        $"  unhandled_top: \"($top_unhandled)\""
    ] | str join "\n"

    # Read existing state, strip any previous tool_usage block, append new one
    let existing = if ($state_path | path exists) {
        open --raw $state_path | str trim
    } else {
        ""
    }

    let stripped = (
        $existing
        | split row "\n"
        | take while {|line| not ($line | str starts-with "tool_usage:") }
        | str join "\n"
        | str trim
    )

    let final = if ($stripped | str length) > 0 {
        $"($stripped)\n($block)\n"
    } else {
        $"($block)\n"
    }

    $final | save --force $state_path
}
```

Run: `nu -c 'source scripts/enrich-handoff.nu'` (syntax check only — no side effects from source)
Expected: exits 0, no errors

- [ ] **Step 2: Run actual smoke test**

```bash
cd /Users/joe/dev/coursers && nu scripts/enrich-handoff.nu --since 1
```

Expected: exits 0. If `rtk` and `handoff-detect` are on PATH, `.ctx/HANDOFF.tools.yaml` and
updated `.ctx/HANDOFF.state.yaml` are written. Inspect:

```bash
# Read tool: /Users/joe/dev/coursers/.ctx/HANDOFF.tools.yaml
# Read tool: /Users/joe/dev/coursers/.ctx/HANDOFF.state.yaml
```

Verify `tool_usage:` block appears in state file and `top_supported:` list in tools file.

- [ ] **Step 3: Commit**

```bash
git add scripts/enrich-handoff.nu
git commit -m "feat(scripts): add enrich-handoff.nu — RTK tool usage enrichment (nu)"
```

---

## Task 2: Write `scripts/enrich-handoff.sh` (POSIX fallback)

**Files:**
- Create: `scripts/enrich-handoff.sh`

- [ ] **Step 1: Write the script**

```sh
#!/bin/sh
# enrich-handoff.sh — POSIX fallback for enrich-handoff.nu
# Usage: sh scripts/enrich-handoff.sh [--since N]
# Writes .ctx/HANDOFF.tools.yaml and merges tool_usage into .ctx/HANDOFF.state.yaml

set -e

SINCE=1
while [ "$#" -gt 0 ]; do
    case "$1" in
        --since) SINCE="$2"; shift 2 ;;
        *) shift ;;
    esac
done

# rtk must be on PATH
if ! command -v rtk >/dev/null 2>&1; then
    exit 0
fi

# handoff-detect must be on PATH
if ! command -v handoff-detect >/dev/null 2>&1; then
    exit 0
fi

ROOT=$(handoff-detect --root 2>/dev/null) || exit 0
CTX="$ROOT/.ctx"
mkdir -p "$CTX"

TOOLS_FILE="$CTX/HANDOFF.tools.yaml"
STATE_FILE="$CTX/HANDOFF.state.yaml"

# Run rtk discover, capture JSON to temp file
TMP=$(mktemp)
trap 'rm -f "$TMP"' EXIT

if ! rtk discover --format json --since "$SINCE" >"$TMP" 2>/dev/null; then
    exit 0
fi

# Parse JSON with python3 (available on all macOS/Linux)
if ! command -v python3 >/dev/null 2>&1; then
    exit 0
fi

TODAY=$(date +%Y-%m-%d)

python3 - "$TMP" "$TOOLS_FILE" "$STATE_FILE" "$TODAY" "$SINCE" <<'PYEOF'
import sys, json, os

tmp, tools_path, state_path, today, since = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5]

with open(tmp) as f:
    data = json.load(f)

supported = data.get("supported", [])[:10]
unsupported = data.get("unsupported", [])[:10]

# Write HANDOFF.tools.yaml
lines = [
    f"generated: {today}",
    f"since_days: {since}",
    f"sessions_scanned: {data.get('sessions_scanned', 0)}",
    f"total_commands: {data.get('total_commands', 0)}",
    "top_supported:",
]
for r in supported:
    lines += [
        f"  - command: {r['command']}",
        f"    count: {r['count']}",
        f"    rtk_equivalent: {r['rtk_equivalent']}",
        f"    est_savings_tokens: {r['estimated_savings_tokens']}",
        f"    est_savings_pct: {r['estimated_savings_pct']}",
    ]
lines.append("top_unhandled:")
for r in unsupported:
    example = r['example'].replace('"', '\\"')
    lines += [
        f"  - base_command: {r['base_command']}",
        f"    count: {r['count']}",
        f'    example: "{example}"',
    ]
with open(tools_path, "w") as f:
    f.write("\n".join(lines) + "\n")

# Build tool_usage block
top_cmd = f"{supported[0]['command']} ({supported[0]['count']})" if supported else ""
total_savings = sum(r['estimated_savings_tokens'] for r in supported)
top_unhandled = f"{unsupported[0]['base_command']} ({unsupported[0]['count']})" if unsupported else ""

block_lines = [
    "tool_usage:",
    f"  sessions_scanned: {data.get('sessions_scanned', 0)}",
    f"  total_commands: {data.get('total_commands', 0)}",
    f'  top_command: "{top_cmd}"',
    f"  est_savings_tokens: {total_savings}",
    f'  unhandled_top: "{top_unhandled}"',
]
block = "\n".join(block_lines)

# Read existing state, strip old tool_usage block, append new one
existing = ""
if os.path.exists(state_path):
    with open(state_path) as f:
        existing = f.read().strip()

stripped_lines = []
for line in existing.splitlines():
    if line.startswith("tool_usage:"):
        break
    stripped_lines.append(line)
stripped = "\n".join(stripped_lines).strip()

final = (stripped + "\n" + block + "\n") if stripped else (block + "\n")
with open(state_path, "w") as f:
    f.write(final)

print(f"enrich-handoff: wrote {tools_path} and updated {state_path}")
PYEOF
```

- [ ] **Step 2: Make executable and smoke test**

```bash
chmod +x scripts/enrich-handoff.sh
sh scripts/enrich-handoff.sh --since 1
```

Expected: same output as nu script — `.ctx/HANDOFF.tools.yaml` written,
`tool_usage:` block in `.ctx/HANDOFF.state.yaml`.

Verify the two output files match in structure (keys and nesting identical, values may differ
slightly due to JSON float formatting).

- [ ] **Step 3: Commit**

```bash
git add scripts/enrich-handoff.sh
git commit -m "feat(scripts): add enrich-handoff.sh — POSIX fallback for RTK enrichment"
```

---

## Task 3: Update `hand:off` skill to call enrich script

**Files:**
- Modify: `~/.claude/plugins/hand/hand/skills/handoff/skill.md`

The existing step 5 ("Write .ctx/HANDOFF.state.yaml") needs a new step 5b inserted after it,
before step 6 (doob sync).

- [ ] **Step 1: Read current skill**

Read `~/.claude/plugins/hand/hand/skills/handoff/skill.md` lines 155–175 to see the exact
text around step 5 and step 6.

- [ ] **Step 2: Insert step 5b**

After the paragraph that ends with "Overwrite completely with current state from step 1.",
insert:

```markdown
### 5b. Enrich with tool usage data

Run the enrich script to write `.ctx/HANDOFF.tools.yaml` and add a `tool_usage` summary
block to `.ctx/HANDOFF.state.yaml`. Use `nu` if available, fall back to `sh`:

```bash
if command -v nu >/dev/null 2>&1; then
  nu "$(git rev-parse --show-toplevel)/scripts/enrich-handoff.nu" 2>/dev/null || true
else
  sh "$(git rev-parse --show-toplevel)/scripts/enrich-handoff.sh" 2>/dev/null || true
fi
```

Non-blocking: if `rtk` is absent or the script fails, continue without error.
```

- [ ] **Step 3: Verify edit looks correct**

Re-read the modified file. Confirm step numbering flows: …5. Write state → 5b. Enrich → 6. Sync to doob…

- [ ] **Step 4: Commit**

```bash
cd ~/.claude/plugins/hand
git add hand/skills/handoff/skill.md
git commit -m "feat(handoff): call enrich-handoff script after state write (step 5b)"
```

---

## Task 4: Update `hand:on` skill to surface tool usage summary

**Files:**
- Modify: `~/.claude/plugins/hand/hand/skills/handon/skill.md`

- [ ] **Step 1: Locate insertion point**

Read `~/.claude/plugins/hand/hand/skills/handon/skill.md`. Find the section after step 2
("Pull latest state from doob") and before step 3 ("Review on wake"). This is where the
`tool_usage` summary belongs — it's orientation data, not a task to act on.

- [ ] **Step 2: Insert new step 2b**

After the doob sync block and before "### 3. Review on wake", insert:

```markdown
### 2b. Surface tool usage summary

After reading `.ctx/HANDOFF.state.yaml`, check for a `tool_usage` key. If present, emit
a one-line summary before proceeding:

```
Tool usage (last session): <total_commands> commands · ~<est_savings_tokens_rounded> tokens
saveable · top unhandled: <unhandled_top>
```

Example:
```
Tool usage (last session): 414 commands · ~19K tokens saveable · top unhandled: op account (19)
```

Round `est_savings_tokens` to nearest K when ≥1000 (e.g. 19217 → ~19K).

If `.ctx/HANDOFF.tools.yaml` also exists, add one line:
```
  Full tool usage detail in .ctx/HANDOFF.tools.yaml
```

Skip silently if `tool_usage` key is absent from state.
```

- [ ] **Step 3: Verify**

Re-read the modified file. Confirm step order: 2. Pull doob → 2b. Tool usage → 3. Review on wake → 4. Parse items…

- [ ] **Step 4: Commit**

```bash
cd ~/.claude/plugins/hand
git add hand/skills/handon/skill.md
git commit -m "feat(handon): surface tool_usage summary after state read (step 2b)"
```

---

## Task 5: Update `hand:over` skill to add Tool Usage section

**Files:**
- Modify: `~/.claude/plugins/hand/hand/skills/handover/skill.md`

- [ ] **Step 1: Read current output structure section**

Read `~/.claude/plugins/hand/hand/skills/handover/skill.md` lines 59–85 (the "Output
Structure" ordered list). The new Tool Usage section goes after section 4 (Log), before
Diagrams.

- [ ] **Step 2: Add Tool Usage to output structure list**

In the numbered output sections list, insert after "4. **Log**":

```markdown
5. **Tool Usage** — summary table + bar chart (only if `.ctx/HANDOFF.tools.yaml` exists)
6. **Diagrams** — all Mermaid diagrams (see below)
```

(renumber the existing "5. **Diagrams**" to 6)

- [ ] **Step 3: Add Tool Usage section spec**

After the "### Review on Wake" section description and before "## Mermaid Diagrams", insert:

```markdown
### Tool Usage

Read `.ctx/HANDOFF.tools.yaml` from the repo root if it exists. If absent, skip this section.

Emit:

**Summary table:**

```markdown
| Metric | Value |
|--------|-------|
| Sessions scanned | <sessions_scanned> |
| Total commands | <total_commands> |
| Top command | <top_supported[0].command> (<top_supported[0].count>x) |
| Est. savings | ~<sum of est_savings_tokens rounded>K tokens |
| Top unhandled | <top_unhandled[0].base_command> (<top_unhandled[0].count>x) |
```

**Mermaid bar chart** (top 5 commands by count from `top_supported` + `top_unhandled` combined,
labeled by command stem):

```mermaid
xychart-beta
  title "Top Commands by Count"
  x-axis ["cargo test", "git -C", "ls", "op account", "cargo nextest"]
  y-axis "Count" 0 --> <max_count_rounded_up>
  bar [69, 118, 42, 19, 13]
```

Rules:
- Use up to 5 entries total: take top 3 from `top_supported`, top 2 from `top_unhandled`
- Truncate command labels to 15 chars max
- Round y-axis max to next multiple of 20 above the highest count
- If fewer than 2 entries total, emit summary table only (no chart)
```

- [ ] **Step 4: Verify**

Re-read the modified file. Confirm Tool Usage section appears between Log and Mermaid Diagrams
in the output structure. Confirm chart rules reference the correct YAML field names
(`top_supported`, `top_unhandled`, `est_savings_tokens`).

- [ ] **Step 5: Commit**

```bash
cd ~/.claude/plugins/hand
git add hand/skills/handover/skill.md
git commit -m "feat(handover): add Tool Usage section with summary table and xychart"
```

---

## Task 6: End-to-end verification

- [ ] **Step 1: Run enrich script manually and inspect outputs**

```bash
cd /Users/joe/dev/coursers
nu scripts/enrich-handoff.nu --since 1
```

Read `.ctx/HANDOFF.tools.yaml` — verify it contains `generated:`, `since_days: 1`,
`sessions_scanned:`, `total_commands:`, `top_supported:` list, `top_unhandled:` list.

Read `.ctx/HANDOFF.state.yaml` — verify `tool_usage:` block is present with all 5 fields:
`sessions_scanned`, `total_commands`, `top_command`, `est_savings_tokens`, `unhandled_top`.

- [ ] **Step 2: Verify sh fallback produces identical schema**

```bash
sh scripts/enrich-handoff.sh --since 1
```

Read both output files again. Keys must be identical to nu output (values may vary slightly
due to timing/float formatting — that's fine).

- [ ] **Step 3: Simulate hand:on tool_usage display**

Read `.ctx/HANDOFF.state.yaml`, extract the `tool_usage` block by eye, and confirm the
one-line summary would render as:

```
Tool usage (last session): <N> commands · ~<K>K tokens saveable · top unhandled: <cmd> (<n>)
```

- [ ] **Step 4: Simulate hand:over Tool Usage section**

Read `.ctx/HANDOFF.tools.yaml`, manually construct the summary table and confirm the
5 x-axis entries and bar values are correct for the chart.

- [ ] **Step 5: Final commit in coursers repo**

```bash
cd /Users/joe/dev/coursers
git add scripts/enrich-handoff.nu scripts/enrich-handoff.sh
git status  # confirm only scripts are staged, no .ctx/ files
git commit -m "feat(scripts): enrich-handoff scripts — RTK tool usage enrichment"
```

(If scripts were committed individually in Tasks 1-2, this step is a no-op — just verify
`git status` is clean.)
