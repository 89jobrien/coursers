# Shared Trait Extraction Plan (coursers + godmode -> devkit)

## Overview

Extract common patterns from `crs-core` and `godmode-core` into `devkit`
(or a new micro-crate) to reduce duplication and establish shared contracts.

This is a separate plan because it couples release cadences and should only
proceed when both projects are actively evolving the affected code.

---

## Candidates for extraction

### 1. JSONL append/read helpers

**Current state**:

- `crs-core` has ad-hoc JSONL reading in `JsonlCommandSource` and writing
  in capture/insights modules
- `godmode-core` has `session_trace.rs` with JSONL append helpers and
  `insights.rs` with JSONL read/write

**Proposed shared type**:

```rust
// devkit::trace

use std::io::{BufRead, Write};
use std::path::Path;

/// Append a single JSON record to a JSONL file (create if missing).
pub fn append_jsonl<T: serde::Serialize>(path: &Path, record: &T)
    -> std::io::Result<()>;

/// Read all records from a JSONL file, skipping malformed lines.
pub fn read_jsonl<T: serde::de::DeserializeOwned>(path: &Path)
    -> impl Iterator<Item = T>;

/// Read records with a filter predicate applied per-line (avoids
/// deserializing everything).
pub fn read_jsonl_filtered<T, F>(path: &Path, filter: F)
    -> impl Iterator<Item = T>
where
    T: serde::de::DeserializeOwned,
    F: Fn(&str) -> bool;
```

**Effort**: S
**Risk**: Low — pure utility, no domain coupling

### 2. Subprocess runner with graceful failure

**Current state**:

- `godmode-core/src/integrations/subprocess.rs` — `run(cmd, args, label)
-> Option<String>`
- `crs-core` uses `std::process::Command` directly in multiple places

**Proposed shared type**:

```rust
// devkit::subprocess

pub struct RunResult {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

/// Run a command, returning None if the binary is not found.
/// Logs via tracing on failure.
pub fn run(cmd: &str, args: &[&str]) -> Option<RunResult>;

/// Check if a binary exists on PATH without running it.
pub fn on_path(cmd: &str) -> bool;
```

**Effort**: S
**Risk**: Low — but godmode's current impl logs to stderr, coursers is
silent. Need to agree on tracing vs silent semantics.

### 3. CommandRecord type

**Current state**:

- `crs-core::analyze::history::CommandRecord` — command, session_id, cwd,
  timestamp, output_bytes
- `godmode-core` doesn't have an equivalent struct but its trace events
  carry similar fields (command, cwd, timestamp)

**Proposed shared type**:

```rust
// devkit::trace

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRecord {
    pub command: String,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_bytes: Option<usize>,
}
```

**Effort**: M
**Risk**: Medium — coursers' `CommandRecord` is used in the `CommandSource`
trait boundary. Changing it means updating `JsonlCommandSource` and all
test fakes. Worth it only if godmode also adopts the same type.

### 4. State file atomic write

**Current state**:

- `crs-core::store::FsStateStore` — write to `.tmp`, rename
- `godmode-core::graph` — similar atomic write pattern
- `godmode-core::pipeline` — same pattern again

**Proposed shared type**:

```rust
// devkit::fs

/// Atomically write content to a file via tmp+rename.
pub fn atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()>;

/// Atomically write a serializable value as JSON.
pub fn atomic_write_json<T: Serialize>(path: &Path, val: &T)
    -> std::io::Result<()>;

/// Atomically write a serializable value as YAML.
pub fn atomic_write_yaml<T: Serialize>(path: &Path, val: &T)
    -> std::io::Result<()>;
```

**Effort**: S
**Risk**: Low — pure utility

---

## Implementation strategy

### Option A: Add to existing `devkit` crate

- `devkit` already exists at `~/dev/devkit`
- Add `trace`, `subprocess`, `fs` modules
- Both `crs-core` and `godmode-core` add `devkit` as a dependency
- Pro: single shared crate, already published
- Con: couples release cadence of 3 projects

### Option B: New micro-crate `forge-common`

- Standalone crate with zero domain logic
- Only utility types and IO helpers
- Pro: minimal coupling, can version independently
- Con: yet another crate to maintain

### Recommendation

**Option A** — add to `devkit`. The candidates are all small utilities that
belong in a toolkit crate. The release coupling is acceptable because these
are stable, low-churn functions.

---

## Sequencing

| Step | What                                                | Depends on                           |
| ---- | --------------------------------------------------- | ------------------------------------ |
| 1    | Extract JSONL helpers into `devkit::trace`          | nothing                              |
| 2    | Extract atomic write into `devkit::fs`              | nothing                              |
| 3    | Extract subprocess runner into `devkit::subprocess` | agree on tracing semantics           |
| 4    | Extract `CommandRecord` into `devkit::trace`        | steps 1-2 done, both projects tested |
| 5    | Update `crs-core` to use `devkit` types             | steps 1-4                            |
| 6    | Update `godmode-core` to use `devkit` types         | steps 1-4                            |

Steps 1-2 can be done in parallel. Step 3 needs a design decision.
Steps 5-6 are independent of each other.

---

## When to do this

**Not yet.** This plan should execute after the 4-phase integration plan is
complete. The integration work will validate whether the shared patterns are
truly stable before extracting them. Premature extraction risks designing
the wrong abstraction.

Trigger conditions:

- Both projects have shipped the integration phases
- A third project (e.g., `doob`, `hj`) would also benefit from the same
  helpers
- A breaking change to JSONL format or subprocess semantics is planned
