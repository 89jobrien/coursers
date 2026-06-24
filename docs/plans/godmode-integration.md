# Godmode x Coursers Integration Plan

## Overview

Four integration points between `godmode` (task orchestration) and `coursers`
(command blocking/rewriting hooks). Ordered by implementation sequence.

---

## Phase 1: Verify gate (`godmode verify` invokes `crs validate`)

**Owner**: godmode
**Effort**: S (1-2 hours)
**Files touched**:

- `godmode-core/src/verify.rs` — new `CrsValidateStep`
- `godmode-core/src/config.rs` — add `coursers: bool` to `[integrations]`

### Design

```rust
pub struct CrsValidateStep;

impl VerifyStep for CrsValidateStep {
    fn name(&self) -> &str { "crs-validate" }

    fn run(&self, root: &Path, _crate_name: Option<&str>) -> Result<StepResult> {
        // Shell out to `crs validate`
        // Parse exit code: 0 = all rules healthy, non-zero = broken
        // Degrade gracefully if `crs` not on PATH
    }
}
```

### Behavior

- `godmode verify` runs `crs validate` as the last step
- Gated by `.godmode.toml` → `integrations.coursers = true` (default false)
- If `crs` binary not found, step is skipped with `ok: true` and a note
- Non-zero exit from `crs validate` fails the verify gate

### Acceptance criteria

- [ ] `godmode verify` in a repo with `integrations.coursers = true` runs
      `crs validate` and reports pass/fail
- [ ] Missing `crs` binary does not break `godmode verify`
- [ ] Broken rule pattern causes verify to fail

---

## Phase 2: Failure data surfacing (`godmode handon` reads coursers state)

**Owner**: godmode
**Effort**: S (2-3 hours)
**Files touched**:

- `godmode-core/src/integrations/coursers.rs` — new module
- `godmode-core/src/integrations/mod.rs` — register module
- `godmode-core/src/integrations/mod.rs` (`handon()`) — add coursers section
- `godmode-core/src/integrations/output.rs` — add `CoursersOut` to `HandonOutput`
- `godmode-core/src/context.rs` — add `coursers_failures` to `SessionContext`

### Design

```rust
// integrations/coursers.rs

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct FailingSummary {
    pub command_preview: String,
    pub count: usize,
    pub last_seen_ago_secs: u64,
}

/// Read coursers failure-learning state from project-local or global path.
/// Returns empty vec on missing/malformed file.
pub fn failing_commands(root: &Path) -> Vec<FailingSummary> {
    let local = root.join(".ctx/course-correct-state.json");
    let global = dirs::home_dir()
        .unwrap_or_default()
        .join(".config/coursers/course-correct-state.json");
    let path = if local.exists() { local } else { global };
    // Parse State, filter to entries above threshold, map to summary
}
```

### Data contract

Reads `course-correct-state.json` directly (no subprocess). The file format
is stable — `State { failures: HashMap<String, FailureEntry> }` where each
`FailureEntry` has `command_preview`, `timestamps: Vec<u64>`, `last_seen: f64`.

### Behavior

- `godmode handon` appends a "Failing commands (coursers)" section when any
  command has 3+ failures in the last hour
- `godmode context --json` includes `coursers_failures: [...]`
- Gated by `integrations.coursers = true`
- Missing or malformed state file returns empty — never errors

### Acceptance criteria

- [ ] `godmode handon` shows failing commands when state file has entries
- [ ] `godmode context --json` includes `coursers_failures` array
- [ ] Missing state file produces no output, no error
- [ ] Entries older than the failure-learning window are excluded

---

## Phase 3: Rule lifecycle pipeline

**Owner**: godmode (pipeline definition + skills)
**Effort**: M (half day)
**Files touched**:

- `godmode/pipelines/coursers-rules.yaml` — pipeline definition
- `godmode/skills/crs-discover/` — discover skill
- `godmode/skills/crs-validate/` — validate skill (reuses `crs validate`)
- `godmode/skills/crs-install/` — install skill

### Pipeline definition

```yaml
name: coursers-rules
description: Discover unhandled commands and propose new blocking rules
steps:
  - skill: crs-discover
  - skill: crs-propose-rules
    optional: true
  - skill: crs-validate
  - skill: crs-install
```

### Skill responsibilities

| Skill               | Input           | Output                                  | Tool                                                      |
| ------------------- | --------------- | --------------------------------------- | --------------------------------------------------------- |
| `crs-discover`      | session history | `.ctx/coursers/crs-candidates.json`     | `crs discover --format json --min-count 3`                |
| `crs-propose-rules` | candidates JSON | `.ctx/coursers/crs-proposed-rules.json` | heuristic or LLM-assisted                                 |
| `crs-validate`      | proposed rules  | pass/fail                               | `crs validate` with merged ruleset                        |
| `crs-install`       | validated rules | updated config                          | merge into `~/.config/coursers/course-correct-rules.json` |

### Flow

```
godmode pipeline start coursers-rules
  -> crs-discover: scans last 30 days, writes candidates
  -> crs-propose-rules: generates rule stubs (pattern, tool, message)
  -> crs-validate: merges proposed + existing, runs validation
  -> crs-install: appends validated rules to config
godmode pipeline status  # shows progress
```

### Acceptance criteria

- [ ] `godmode pipeline start coursers-rules` runs all 4 steps sequentially
- [ ] Candidates with <3 occurrences are filtered out
- [ ] `crs-validate` failure stops the pipeline (does not install bad rules)
- [ ] Installed rules appear in `crs validate` output on next run

---

## Phase 4: Task-aware rule overrides (coursers reads godmode context)

**Owner**: coursers
**Effort**: M (half day)
**Files touched**:

- `crs-core/src/config.rs` — add godmode cache path resolution
- `crs-core/src/rules.rs` — add `task_override` field to `Rule`
- `crs-core/Cargo.toml` — no new deps (just reads JSON)
- `crates/coursers/src/hook/mod.rs` — wire override check into pre-hook

### Design

```rust
// In Rule definition (rules.json)
{
  "id": "no-grep",
  "pattern": "\\bgrep\\b",
  "tool": "Grep",
  "message": "Use the Grep tool instead",
  "task_override": "grep*"  // <-- new optional field
}
```

```rust
// crs-core/src/rules.rs
pub struct Rule {
    // ...existing fields...
    #[serde(default)]
    pub task_override: Option<String>,
}

/// Check if any running godmode task title matches the override glob.
pub fn task_overrides_rule(rule: &Rule, running_titles: &[String]) -> bool {
    let Some(pattern) = &rule.task_override else { return false };
    running_titles.iter().any(|t| glob_match(pattern, t))
}
```

### Data source

Reads `~/.cache/godmode/status.json` — a file godmode already writes on
every status change. Format:

```json
{
  "running": ["[t1] migrate grep to rg", "[t2] add filter tests"],
  "pending": 3,
  "blocked": 1
}
```

No subprocess call. File read is <1ms. If file is missing or stale, no
overrides apply — the rule fires normally.

### Performance constraint

The pre-hook hot path must stay under 5ms. Reading a small JSON file and
matching a glob against 0-5 running task titles adds <0.5ms. Acceptable.

### Acceptance criteria

- [ ] Rule with `task_override: "migrate*"` is suppressed when a running
      task title matches
- [ ] Missing godmode cache file has no effect (rule fires normally)
- [ ] `crs probe` shows override status per rule
- [ ] No new dependencies added to `crs-core`

---

## Dependency graph

```
Phase 1 (verify gate)
  |
  v
Phase 2 (failure surfacing) --- no dependency on Phase 1, but same
  |                              integration config toggle
  v
Phase 3 (pipeline) --- depends on `crs discover --format json` existing
  |                    (already shipped)
  v
Phase 4 (task overrides) --- depends on godmode status cache format
                             being stable (already shipped)
```

Phases 1-2 can be done in parallel. Phase 3 depends on Phase 1 only for
the shared `integrations.coursers` config flag. Phase 4 is independent
of the godmode-side work.
