# Test Harness Design — coursers

**Date:** 2026-04-06
**Status:** approved

---

## Overview

A full test harness for the `coursers` workspace covering:

1. **Core refactor** — extract `RulesLoader` and `StateStore` traits so domain logic is testable
   without filesystem I/O
2. **Unit tests** — inline `#[cfg(test)]` modules in `crates/core` and `crates/coursers`
3. **Integration tests** — workspace-level crate that spawns real binaries via
   `std::process::Command`
4. **Smoke script** — `scripts/smoke.nu` for manual end-to-end validation against the live config

---

## Section 1: Core Refactor — Trait Extraction

### New traits in `crates/core`

**`crs_core::loader::RulesLoader`**

```rust
pub trait RulesLoader {
    fn load(&self) -> RulesConfig;
}
```

Real implementation: `FsRulesLoader` — reads from `COURSERS_RULES` or the default path.
Test double: `InMemoryRulesLoader(RulesConfig)` — returns a fixed config.

**`crs_core::store::StateStore`**

```rust
pub trait StateStore {
    fn load(&self) -> State;
    fn save(&self, state: &State);
    fn path(&self) -> &std::path::Path;
}
```

Real implementation: `FsStateStore` — reads/writes JSON to disk.
Test double: `InMemoryStateStore` — `RefCell<State>`, no I/O.

### Hook function signatures

`hook::pre` and `hook::post` expose a `run_with` function for testing:

```rust
// hook/pre.rs
pub fn run_with<L: RulesLoader, S: StateStore>(loader: &L, store: &S, payload: &Payload);

// hook/post.rs
pub fn run_with<L: RulesLoader, S: StateStore>(loader: &L, store: &S, payload: &Payload);
```

`main.rs` calls these with `FsRulesLoader` and `FsStateStore`. The existing `run()` entry
points become thin wrappers that read stdin and call `run_with`.

### Test doubles location

`crs_core::testing` module, gated behind a `testing` Cargo feature:

```toml
[features]
testing = []
```

`crates/coursers` enables `crs_core/testing` in `[dev-dependencies]`.

---

## Section 2: Unit Tests

### `crates/core/src/rules.rs`

| Test                            | Assertion                                |
| ------------------------------- | ---------------------------------------- |
| `rule_matches_pattern`          | matching command → `Some(message)`       |
| `rule_no_match`                 | non-matching command → `None`            |
| `rule_case_insensitive_flag`    | `pattern_flags = "i"` matches mixed case |
| `rule_exception_bypasses_block` | exception regex present → `None`         |
| `rule_disabled_skipped`         | `enabled: false` → `None`                |
| `rule_bad_regex_skipped`        | invalid pattern → `None` (no panic)      |
| `no_rules_allows_all`           | empty rules vec → `None`                 |

### `crates/core/src/state.rs`

| Test                            | Assertion                            |
| ------------------------------- | ------------------------------------ |
| `record_failure_creates_entry`  | new command → entry with 1 timestamp |
| `record_failure_appends`        | same command twice → 2 timestamps    |
| `prune_removes_old_timestamps`  | timestamp outside window → pruned    |
| `prune_evicts_over_max`         | 201 entries → oldest evicted         |
| `check_learned_below_threshold` | 2 failures, threshold 3 → `None`     |
| `check_learned_at_threshold`    | 3 failures → `Some(message)`         |
| `check_learned_disabled`        | `enabled: false` → `None`            |
| `cleanup_removes_stale_entries` | last_seen > cleanup_after → removed  |

### `crates/core/src/config.rs`

| Test                                   | Assertion                                  |
| -------------------------------------- | ------------------------------------------ |
| `env_var_overrides_default_rules_path` | `COURSERS_RULES=/tmp/x` → path is `/tmp/x` |
| `default_rules_path_contains_claude`   | no env var → path contains `.claude`       |

### `crates/coursers/src/hook/pre.rs`

Uses `InMemoryRulesLoader` + `InMemoryStateStore`.

| Test                                  | Assertion                                      |
| ------------------------------------- | ---------------------------------------------- |
| `non_bash_tool_passthrough`           | `tool_name = "Read"` → no deny                 |
| `rule_match_denies`                   | command matches rule → deny response           |
| `exception_allows`                    | command matches exception → allow              |
| `learned_failure_at_threshold_denies` | state has N failures → deny                    |
| `empty_command_passthrough`           | blank command → no deny                        |
| `failure_learning_disabled_allows`    | `fl.enabled = false` → allow even at threshold |

### `crates/coursers/src/hook/post.rs`

| Test                                  | Assertion                                   |
| ------------------------------------- | ------------------------------------------- |
| `exit_zero_no_record`                 | exit_code 0 → state unchanged               |
| `signal_exit_no_record`               | exit_code 130 → state unchanged             |
| `excluded_pattern_no_record`          | `2>/dev/null` suffix → not recorded         |
| `real_failure_recorded`               | exit_code 1, plain command → entry in state |
| `failure_learning_disabled_no_record` | `fl.enabled = false` → not recorded         |

---

## Section 3: Integration Tests

### Structure

```
tests/
  integration/
    fixtures/
      rules_basic.json         — two rules: no-grep, no-cat
      rules_empty.json         — { "rules": [] }
      payload_bash_grep.json   — PreToolUse Bash "grep foo ."
      payload_bash_ls.json     — PreToolUse Bash "ls"
      payload_non_bash.json    — PreToolUse tool_name="Read"
      payload_post_fail.json   — PostToolUse exit_code=1, command="grep foo ."
      payload_post_ok.json     — PostToolUse exit_code=0
      payload_post_signal.json — PostToolUse exit_code=130
    pre_hook.rs
    post_hook.rs
```

`Cargo.toml` workspace addition:

```toml
[[test]]
name = "integration"
path = "tests/integration/pre_hook.rs"
```

Binary path resolved via:

```rust
let bin = env!("CARGO_BIN_EXE_coursers");
```

Each test uses `tempfile::TempDir` for the state file and sets `COURSERS_RULES` to a fixture
path via `.env("COURSERS_RULES", ...)` on `Command`.

### `tests/integration/pre_hook.rs`

| Test                                     | Setup                                      | Assertion                              |
| ---------------------------------------- | ------------------------------------------ | -------------------------------------- |
| `blocked_command_exits_nonzero`          | rules_basic + payload_bash_grep            | exit non-zero, stdout contains `block` |
| `allowed_command_exits_zero`             | rules_basic + payload_bash_ls              | exit 0                                 |
| `non_bash_passthrough`                   | rules_basic + payload_non_bash             | exit 0                                 |
| `learned_failure_blocks_after_threshold` | empty rules, post failure N times then pre | pre exits non-zero                     |

### `tests/integration/post_hook.rs`

| Test                            | Setup                               | Assertion                         |
| ------------------------------- | ----------------------------------- | --------------------------------- |
| `failure_recorded_in_state`     | payload_post_fail                   | state file exists, contains entry |
| `success_not_recorded`          | payload_post_ok                     | state file empty or absent        |
| `signal_not_recorded`           | payload_post_signal                 | state file empty or absent        |
| `excluded_pattern_not_recorded` | post payload with `cmd 2>/dev/null` | not recorded                      |

---

## Section 4: Smoke Script

**`scripts/smoke.nu`** — manual end-to-end validation against the real binary and live config.

```
Usage: nu scripts/smoke.nu
```

Behavior:

1. Resolves `coursers` binary via `which coursers` (falls back to `./target/release/coursers`)
2. Uses live `~/.claude/hooks/course-correct-rules.json`
3. Creates a temp state file so real failure state is not polluted
4. Runs four scenarios:
   - Send a should-block payload → assert stdout contains `block`
   - Send a should-allow payload → assert exit 0
   - Send post failure payload 3× → send pre payload → assert blocked
   - Send post exit-0 payload → assert state file unchanged
5. Prints a pass/fail table

---

## File Map

| File                                | Action                                                            |
| ----------------------------------- | ----------------------------------------------------------------- |
| `crates/core/src/loader.rs`         | new — `RulesLoader` trait + `FsRulesLoader`                       |
| `crates/core/src/store.rs`          | new — `StateStore` trait + `FsStateStore`                         |
| `crates/core/src/testing.rs`        | new — `InMemoryRulesLoader`, `InMemoryStateStore` (feature-gated) |
| `crates/core/src/lib.rs`            | add `pub mod loader`, `pub mod store`, `pub mod testing`          |
| `crates/core/src/rules.rs`          | unit tests added                                                  |
| `crates/core/src/state.rs`          | unit tests added                                                  |
| `crates/core/src/config.rs`         | unit tests added                                                  |
| `crates/core/Cargo.toml`            | add `testing` feature                                             |
| `crates/coursers/src/hook/pre.rs`   | refactor to `run_with`, unit tests added                          |
| `crates/coursers/src/hook/post.rs`  | refactor to `run_with`, unit tests added                          |
| `crates/coursers/src/main.rs`       | wire `FsRulesLoader` + `FsStateStore`                             |
| `crates/coursers/Cargo.toml`        | add `crs_core/testing` to dev-dependencies                        |
| `tests/integration/pre_hook.rs`     | new                                                               |
| `tests/integration/post_hook.rs`    | new                                                               |
| `tests/integration/fixtures/*.json` | new                                                               |
| `Cargo.toml` (workspace)            | add `tempfile` to workspace dev-dependencies                      |
| `scripts/smoke.nu`                  | new                                                               |
| `hooks/`                            | deleted                                                           |
| `src/`                              | staged deletion committed                                         |
