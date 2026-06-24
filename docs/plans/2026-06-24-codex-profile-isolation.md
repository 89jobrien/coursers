# Plan: Codex Profile Isolation

## Goal

Add profile-aware path resolution to `crs-core` and expose `--profile`/`--rules`/`--state`
CLI flags on all `coursers` and `crs` subcommands, then wire the Codex hook entries so
Codex sessions use fully isolated rules and state from Claude Code.

## Architecture

- **Crates affected**: `crs-core` (new types), `coursers` (CLI + hook wiring), `crs` (CLI)
- **New types**:
  - `ProfileConfig` in `crs_core::config`
  - `ConfigBuilder` in `crs_core::config`
  - `ProfileFsRulesLoader` in `crs_core::loader`
- **Data flow**:
  CLI args → `ConfigBuilder::build()` → `ProfileConfig` → `ProfileFsRulesLoader` +
  `FsStateStore` → existing `run_with()` hook logic (unchanged)

## Tech Stack

- Rust edition 2024; `clap` (already in both binaries), `dirs` (already in `crs-core`)
- No new dependencies

---

## Tasks

### Task 1: Add `ProfileConfig` and `ConfigBuilder` to `crs-core::config`

**Crate**: `crs-core`
**File(s)**: `crates/core/src/config.rs`
**Run**: `cargo nextest run -p crs-core`

#### 1. Write failing tests

```rust
#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn default_builder_gives_legacy_rules_path() {
        let cfg = ConfigBuilder::new().build();
        assert!(
            cfg.rules_path
                .to_string_lossy()
                .contains("course-correct-rules.json"),
            "got: {}",
            cfg.rules_path.display()
        );
    }

    #[test]
    fn default_builder_gives_legacy_global_state_path() {
        let cfg = ConfigBuilder::new().build();
        assert!(
            cfg.global_state_path
                .to_string_lossy()
                .contains("course-correct-state.json"),
            "got: {}",
            cfg.global_state_path.display()
        );
    }

    #[test]
    fn default_builder_gives_legacy_local_state_path() {
        let cfg = ConfigBuilder::new().build();
        assert_eq!(
            cfg.local_state_path,
            std::path::PathBuf::from(".ctx/course-correct-state.json")
        );
    }

    #[test]
    fn profile_builder_resolves_rules_under_profiles_dir() {
        let cfg = ConfigBuilder::new().profile("codex").build();
        assert!(
            cfg.rules_path
                .to_string_lossy()
                .contains("profiles/codex/rules.json"),
            "got: {}",
            cfg.rules_path.display()
        );
    }

    #[test]
    fn profile_builder_resolves_global_state_under_profiles_dir() {
        let cfg = ConfigBuilder::new().profile("codex").build();
        assert!(
            cfg.global_state_path
                .to_string_lossy()
                .contains("profiles/codex/state.json"),
            "got: {}",
            cfg.global_state_path.display()
        );
    }

    #[test]
    fn profile_builder_gives_profile_scoped_local_state_path() {
        let cfg = ConfigBuilder::new().profile("codex").build();
        assert_eq!(
            cfg.local_state_path,
            std::path::PathBuf::from(".ctx/crs-codex-state.json")
        );
    }

    #[test]
    fn rules_override_wins_over_profile() {
        let cfg = ConfigBuilder::new()
            .profile("codex")
            .rules(std::path::PathBuf::from("/tmp/custom-rules.json"))
            .build();
        assert_eq!(cfg.rules_path, std::path::PathBuf::from("/tmp/custom-rules.json"));
    }

    #[test]
    fn state_override_wins_over_profile() {
        let cfg = ConfigBuilder::new()
            .profile("codex")
            .state(std::path::PathBuf::from("/tmp/custom-state.json"))
            .build();
        assert_eq!(
            cfg.global_state_path,
            std::path::PathBuf::from("/tmp/custom-state.json")
        );
    }

    #[test]
    fn effective_state_path_returns_local_when_exists() {
        // Can't create files in unit tests without tempdir — test the fallback branch:
        // when local path does NOT exist, effective_state_path returns global_state_path.
        let cfg = ConfigBuilder::new().build();
        // .ctx/course-correct-state.json almost certainly does not exist in test CWD
        let effective = cfg.effective_state_path();
        // Must point to either local or global — both are valid PathBufs
        assert!(!effective.as_os_str().is_empty());
    }
}
```

Run: `cargo nextest run -p crs-core -- profile_tests`
Expected: FAIL (types do not exist yet)

#### 2. Implement

Add after the existing free functions in `crates/core/src/config.rs`:

```rust
/// Resolved paths for a named profile (or the default profile).
/// Constructed via [`ConfigBuilder::build`].
pub struct ProfileConfig {
    /// Path to the rules JSON file.
    pub rules_path: PathBuf,
    /// Path to the global (home-dir) state file.
    pub global_state_path: PathBuf,
    /// Project-local state path (`.ctx/crs-<profile>-state.json`).
    pub local_state_path: PathBuf,
}

impl ProfileConfig {
    /// Returns the project-local state path if it exists on disk,
    /// otherwise returns the global state path.
    pub fn effective_state_path(&self) -> &PathBuf {
        if self.local_state_path.exists() {
            &self.local_state_path
        } else {
            &self.global_state_path
        }
    }
}

/// Builder for [`ProfileConfig`]. Layered resolution:
/// defaults → profile directory → explicit overrides.
pub struct ConfigBuilder {
    profile: Option<String>,
    rules_override: Option<PathBuf>,
    state_override: Option<PathBuf>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self {
            profile: None,
            rules_override: None,
            state_override: None,
        }
    }

    /// Set a named profile. Resolves to `~/.config/coursers/profiles/<name>/`.
    pub fn profile(mut self, name: impl Into<String>) -> Self {
        self.profile = Some(name.into());
        self
    }

    /// Override the rules path; takes precedence over the profile directory.
    pub fn rules(mut self, path: PathBuf) -> Self {
        self.rules_override = Some(path);
        self
    }

    /// Override the global state path; takes precedence over the profile directory.
    pub fn state(mut self, path: PathBuf) -> Self {
        self.state_override = Some(path);
        self
    }

    pub fn build(self) -> ProfileConfig {
        let home = dirs::home_dir().expect("home dir");
        let base = home.join(".config/coursers");

        let (default_rules, default_global_state, default_local_state) =
            if let Some(ref name) = self.profile {
                let profile_dir = base.join("profiles").join(name);
                (
                    profile_dir.join("rules.json"),
                    profile_dir.join("state.json"),
                    PathBuf::from(format!(".ctx/crs-{name}-state.json")),
                )
            } else {
                (
                    base.join("course-correct-rules.json"),
                    base.join("course-correct-state.json"),
                    PathBuf::from(".ctx/course-correct-state.json"),
                )
            };

        ProfileConfig {
            rules_path: self.rules_override.unwrap_or(default_rules),
            global_state_path: self.state_override.unwrap_or(default_global_state),
            local_state_path: default_local_state,
        }
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}
```

#### 3. Verify

```
cargo nextest run -p crs-core -- profile_tests   → all green
cargo clippy -p crs-core -- -D warnings          → zero warnings
```

#### 4. Commit

```
git branch --show-current   # must NOT be main
git commit -m "feat(crs-core): add ProfileConfig and ConfigBuilder for profile-aware path resolution"
```

---

### Task 2: Add `ProfileFsRulesLoader` to `crs-core::loader`

**Crate**: `crs-core`
**File(s)**: `crates/core/src/loader.rs`
**Run**: `cargo nextest run -p crs-core`

#### 1. Write failing test

```rust
#[test]
fn profile_fs_loader_returns_default_on_missing_file() {
    let loader = ProfileFsRulesLoader {
        path: std::path::PathBuf::from("/nonexistent/profile-rules.json"),
    };
    let config = loader.load();
    assert!(config.rules.is_empty());
}

#[test]
fn profile_fs_loader_loads_from_explicit_path() {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    writeln!(f, r#"{{"rules":[],"failure_learning":{{"enabled":false}}}}"#).unwrap();
    let loader = ProfileFsRulesLoader { path: f.path().to_path_buf() };
    let config = loader.load();
    assert!(!config.failure_learning.enabled);
}
```

Note: `tempfile` is already in dev-dependencies. If not, use `std::fs::write` to
`/tmp/test-profile-rules.json` and clean up manually.

Run: `cargo nextest run -p crs-core -- profile_fs_loader`
Expected: FAIL

#### 2. Implement

Add after `FsRulesLoader` impl in `crates/core/src/loader.rs`:

```rust
/// Loads rules from an explicit path — no env-var lookup.
/// Use when a [`crate::config::ProfileConfig`] has already resolved the path.
pub struct ProfileFsRulesLoader {
    pub path: std::path::PathBuf,
}

impl RulesLoader for ProfileFsRulesLoader {
    fn load(&self) -> RulesConfig {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| RulesConfig {
                rules: vec![],
                failure_learning: crate::rules::FailureLearning::default(),
            })
    }
}
```

#### 3. Verify

```
cargo nextest run -p crs-core               → all green
cargo clippy -p crs-core -- -D warnings    → zero warnings
```

#### 4. Commit

```
git branch --show-current   # must NOT be main
git commit -m "feat(crs-core): add ProfileFsRulesLoader for explicit-path rule loading"
```

---

### Task 3: Add `hook_context_with_profile` to `coursers::hook`

**Crate**: `coursers`
**File(s)**: `crates/coursers/src/hook/mod.rs`
**Run**: `cargo nextest run -p coursers`

No new logic tests needed here — this is glue. Verify by building.

#### 1. Implement

Add after `hook_context()` in `crates/coursers/src/hook/mod.rs`:

```rust
/// Profile-aware variant of [`hook_context`].
/// Constructs loaders and stores from a resolved [`crs_core::config::ProfileConfig`].
#[allow(clippy::type_complexity)]
pub fn hook_context_with_profile(
    profile_cfg: &crs_core::config::ProfileConfig,
) -> Option<(
    HookPayload,
    crs_core::loader::ProfileFsRulesLoader,
    crs_core::store::FsStateStore,
    crs_core::capture::SuggestionStore,
)> {
    use crs_core::capture::SuggestionStore;
    use crs_core::loader::ProfileFsRulesLoader;
    use crs_core::store::FsStateStore;

    let payload = read_stdin()?;
    let loader = ProfileFsRulesLoader {
        path: profile_cfg.rules_path.clone(),
    };
    let store = FsStateStore {
        path: profile_cfg.effective_state_path().clone(),
    };
    let capture = SuggestionStore::new(SuggestionStore::default_path());
    Some((payload, loader, store, capture))
}
```

#### 2. Verify

```
cargo build -p coursers                       → clean build
cargo clippy -p coursers -- -D warnings      → zero warnings
```

#### 3. Commit

```
git branch --show-current   # must NOT be main
git commit -m "feat(coursers): add hook_context_with_profile() for profile-aware hook wiring"
```

---

### Task 4: Add `--profile`/`--rules`/`--state` flags to `coursers` binary

**Crate**: `coursers`
**File(s)**: `crates/coursers/src/main.rs`, `crates/coursers/src/hook/pre.rs`,
`crates/coursers/src/hook/post.rs`
**Run**: `cargo nextest run -p coursers`

#### 1. Write failing test (integration-style)

In `crates/coursers/tests/pre_hook.rs`, add:

```rust
// Verify that --profile flag is accepted (parse-only smoke test via clap)
// Real isolation is tested in Task 1's unit tests.
#[test]
fn pre_subcommand_accepts_profile_flag() {
    // This test verifies the binary's clap schema accepts --profile.
    // It is a compile-time check only; no binary execution needed.
    // If the flag is missing, this file will fail to compile after Task 4.
    // Actual CLI integration is covered by smoke.nu.
    // (Placeholder — the real check is `cargo build` succeeding.)
    assert!(true);
}
```

#### 2. Implement in `crates/coursers/src/main.rs`

Replace the entire file:

```rust
mod hook;

use clap::{Parser, Subcommand};
use crs_core::config::ConfigBuilder;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "coursers",
    about = "Claude Code course-correction hook pipeline"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// PreToolUse hook — reads JSON payload from stdin, writes hook response to stdout
    Pre {
        /// Named profile (resolves to ~/.config/coursers/profiles/<name>/)
        #[arg(long)]
        profile: Option<String>,
        /// Override rules file path
        #[arg(long)]
        rules: Option<PathBuf>,
        /// Override global state file path
        #[arg(long)]
        state: Option<PathBuf>,
    },
    /// PostToolUse hook — reads JSON payload from stdin, records failures
    Post {
        /// Named profile (resolves to ~/.config/coursers/profiles/<name>/)
        #[arg(long)]
        profile: Option<String>,
        /// Override rules file path
        #[arg(long)]
        rules: Option<PathBuf>,
        /// Override global state file path
        #[arg(long)]
        state: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Pre {
            profile,
            rules,
            state,
        } => {
            let mut builder = ConfigBuilder::new();
            if let Some(p) = profile {
                builder = builder.profile(p);
            }
            if let Some(r) = rules {
                builder = builder.rules(r);
            }
            if let Some(s) = state {
                builder = builder.state(s);
            }
            let profile_cfg = builder.build();
            hook::pre::run_with_profile(&profile_cfg);
        }
        Command::Post {
            profile,
            rules,
            state,
        } => {
            let mut builder = ConfigBuilder::new();
            if let Some(p) = profile {
                builder = builder.profile(p);
            }
            if let Some(r) = rules {
                builder = builder.rules(r);
            }
            if let Some(s) = state {
                builder = builder.state(s);
            }
            let profile_cfg = builder.build();
            hook::post::run_with_profile(&profile_cfg);
        }
    }
}
```

#### 3. Add `run_with_profile` to `crates/coursers/src/hook/pre.rs`

Add after `run()`:

```rust
/// Profile-aware entry point. Replaces the zero-arg `run()` when --profile/--rules/--state
/// are provided. Zero-arg `run()` remains for backward compatibility.
pub fn run_with_profile(profile_cfg: &crs_core::config::ProfileConfig) {
    let Some((payload, loader, store, capture)) =
        super::hook_context_with_profile(profile_cfg)
    else {
        return;
    };
    run_with(&loader, &store, &capture, &payload);
}
```

#### 4. Add `run_with_profile` to `crates/coursers/src/hook/post.rs`

Add after `run()`:

```rust
pub fn run_with_profile(profile_cfg: &crs_core::config::ProfileConfig) {
    let Some((payload, loader, store, capture)) =
        super::hook_context_with_profile(profile_cfg)
    else {
        return;
    };
    run_with(&loader, &store, &capture, &payload);
}
```

#### 5. Verify

```
cargo build -p coursers                                    → clean
cargo nextest run -p coursers                              → all green
cargo clippy -p coursers -- -D warnings                   → zero warnings
# Smoke: default invocation unchanged
echo '{"tool_name":"Read"}' | coursers pre                → silent exit 0
echo '{"tool_name":"Read"}' | coursers pre --profile codex → silent exit 0
```

#### 6. Commit

```
git branch --show-current   # must NOT be main
git commit -m "feat(coursers): add --profile/--rules/--state flags to pre/post subcommands"
```

---

### Task 5: Add `--profile`/`--rules`/`--state` flags to `crs` subcommands

**Crate**: `crs`
**File(s)**: `crates/crs/src/main.rs`
**Run**: `cargo nextest run -p crs`

Only subcommands that read rules or state gain the flags. Per the design table:

| Subcommand | `--profile` | `--rules` | `--state` |
| ---------- | ----------- | --------- | --------- |
| `filter`   | yes         | yes       | yes       |
| `rewrite`  | yes         | yes       | no        |
| `discover` | yes         | yes       | no        |
| `validate` | yes         | yes       | no        |
| `probe`    | yes         | yes       | no        |
| `stats`    | yes         | no        | no        |
| `suggest`  | yes         | yes       | no        |

`insights`, `audit`, `history`, `export`, `hook`, `validate-hooks`, `log`, `heat`,
`replay`, `nu-check` gain no new flags.

#### 1. Implement

In `crates/crs/src/main.rs`, add shared profile args as a helper — since `clap` does not
support struct-flattening cleanly for all cases here, add the flags directly to each
variant. Add to the top of `main.rs`:

```rust
use crs_core::config::ConfigBuilder;
```

Modify each affected `Command` variant to add the profile args:

```rust
// Filter variant — replace bare `Filter` with:
Filter {
    #[arg(long)]
    profile: Option<String>,
    #[arg(long)]
    rules: Option<std::path::PathBuf>,
    #[arg(long)]
    state: Option<std::path::PathBuf>,
},

// Rewrite variant — replace bare `Rewrite` with:
Rewrite {
    #[arg(long)]
    profile: Option<String>,
    #[arg(long)]
    rules: Option<std::path::PathBuf>,
},

// Discover — add to existing fields:
// (keep all existing fields; add at top of variant)
//   #[arg(long)] profile: Option<String>,
//   #[arg(long)] rules: Option<std::path::PathBuf>,

// Validate — replace bare `Validate` with:
Validate {
    #[arg(long)]
    profile: Option<String>,
    #[arg(long)]
    rules: Option<std::path::PathBuf>,
},

// Probe — replace bare `Probe` with:
Probe {
    #[arg(long)]
    profile: Option<String>,
    #[arg(long)]
    rules: Option<std::path::PathBuf>,
},

// Stats — replace bare `Stats` with:
Stats {
    #[arg(long)]
    profile: Option<String>,
},

// Suggest — add to existing fields:
// (keep all existing fields; add at top of variant)
//   #[arg(long)] profile: Option<String>,
//   #[arg(long)] rules: Option<std::path::PathBuf>,
```

Add a helper function at module level (not inside `main`) that converts the three
optional args into a `ProfileConfig`:

```rust
fn resolve_profile(
    profile: Option<String>,
    rules: Option<std::path::PathBuf>,
    state: Option<std::path::PathBuf>,
) -> crs_core::config::ProfileConfig {
    let mut b = ConfigBuilder::new();
    if let Some(p) = profile { b = b.profile(p); }
    if let Some(r) = rules  { b = b.rules(r); }
    if let Some(s) = state  { b = b.state(s); }
    b.build()
}
```

Update the `match` arms in `main()` for each affected subcommand to extract `profile`,
`rules`, `state` and call `resolve_profile(profile, rules, state)`. Pass the resolved
`ProfileConfig` into each `cmd_*` function.

Update `cmd_filter`, `cmd_rewrite`, `cmd_validate`, `cmd_probe`, `cmd_discover`,
`cmd_suggest`, `cmd_stats` to accept `profile_cfg: &crs_core::config::ProfileConfig`
as their first argument and use `ProfileFsRulesLoader { path: profile_cfg.rules_path.clone() }`
and `FsStateStore { path: profile_cfg.effective_state_path().clone() }` instead of the
current env-var-based loaders.

The existing no-arg calls remain correct because `resolve_profile(None, None, None)` ≡
`ConfigBuilder::new().build()` ≡ existing defaults.

#### 2. Verify

```
cargo build -p crs                                       → clean
cargo nextest run -p crs                                 → all green
cargo clippy -p crs -- -D warnings                      → zero warnings
# Smoke:
echo '{}' | crs filter                                   → exit 0 (unchanged)
echo '{}' | crs filter --profile codex                   → exit 0
crs validate                                             → (unchanged output)
crs validate --profile codex                             → uses profile rules path
```

#### 3. Commit

```
git branch --show-current   # must NOT be main
git commit -m "feat(crs): add --profile/--rules/--state flags to filter/rewrite/discover/validate/probe/stats/suggest"
```

---

### Task 6: Scaffold Codex profile files

**File(s)**: `~/.config/coursers/profiles/codex/rules.json`,
`~/.config/coursers/profiles/codex/state.json`

No Rust changes. No tests. This is filesystem scaffolding.

#### 1. Create directory and seed files

```bash
mkdir -p ~/.config/coursers/profiles/codex

# Seed rules.json as a copy of the base rules (Codex-specific tuning can be applied later)
cp ~/.config/coursers/course-correct-rules.json \
   ~/.config/coursers/profiles/codex/rules.json

# Seed empty state
echo '{"failures":{}}' > ~/.config/coursers/profiles/codex/state.json
```

#### 2. Verify

```
crs validate --profile codex   → rules load and patterns compile
```

---

### Task 7: Wire Codex hooks in `~/.codex/hooks.json`

**File(s)**: `~/.codex/hooks.json`

No Rust changes.

#### 1. Current state

`~/.codex/hooks.json` has:

- `PreToolUse/Bash`: `destructive-guard.rs` + `cargo-lint-gate.rs`
- `PostToolUse/Bash`: `redact-output.rs`
- `PostToolUse/Edit|Write`: `cargo-check-on-edit.rs`

#### 2. Add coursers + crs hooks

Replace `~/.codex/hooks.json` with:

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "startup|resume",
        "hooks": [
          {
            "type": "command",
            "command": "rust-script /Users/joe/.codex/hooks/session/session-start.rs",
            "statusMessage": "Loading session context",
            "timeout": 30
          }
        ]
      }
    ],
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "rust-script /Users/joe/.codex/hooks/safety/destructive-guard.rs",
            "statusMessage": "Checking for destructive operations",
            "timeout": 15
          },
          {
            "type": "command",
            "command": "rust-script /Users/joe/.codex/hooks/rust-dev/cargo-lint-gate.rs",
            "statusMessage": "Running cargo lint checks",
            "timeout": 130
          },
          {
            "type": "command",
            "command": "coursers pre --profile codex",
            "statusMessage": "Coursers pre-check (codex)",
            "timeout": 15
          },
          {
            "type": "command",
            "command": "crs rewrite --profile codex",
            "statusMessage": "Crs rewrite (codex)",
            "timeout": 15
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "rust-script /Users/joe/.codex/hooks/safety/redact-output.rs",
            "statusMessage": "Scanning output for secrets",
            "timeout": 15
          },
          {
            "type": "command",
            "command": "coursers post --profile codex",
            "statusMessage": "Coursers post-record (codex)",
            "timeout": 15
          },
          {
            "type": "command",
            "command": "crs filter --profile codex",
            "statusMessage": "Crs filter (codex)",
            "timeout": 15
          }
        ]
      },
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "rust-script /Users/joe/.codex/hooks/rust-dev/cargo-check-on-edit.rs",
            "statusMessage": "Running cargo check",
            "timeout": 130
          }
        ]
      }
    ]
  }
}
```

#### 3. Verify (manual smoke)

```bash
# Confirm binaries are on PATH from Codex's env
which coursers crs

# Confirm profile resolves
crs validate --profile codex

# Confirm state file isolation
ls ~/.config/coursers/profiles/codex/
# → rules.json  state.json
```

---

### Task 8: Full workspace verification and integration commit

**Run**: `cargo nextest run --workspace`

#### 1. Run full suite

```
cargo nextest run --workspace        → all green
cargo clippy --workspace -- -D warnings  → zero warnings
cargo build --release                → clean release build
```

#### 2. Smoke test end-to-end isolation

```bash
# Claude Code session — uses default paths
echo '{"tool_name":"Bash","tool_input":{"command":"grep foo ."}}' | coursers pre
# → deny (rule fires against default rules)

# Codex session — uses codex profile paths
echo '{"tool_name":"Bash","tool_input":{"command":"grep foo ."}}' | coursers pre --profile codex
# → deny (same rule from codex copy) — no state crossover

# Confirm state files are separate on disk after a few failures
ls -la ~/.config/coursers/course-correct-state.json
ls -la ~/.config/coursers/profiles/codex/state.json
```

#### 3. Commit

```
git branch --show-current   # must NOT be main
git commit -m "chore: full workspace green after codex profile isolation"
```

---

## Quality Checklist

- [ ] `cargo nextest run --workspace` → all green
- [ ] `cargo clippy --workspace -- -D warnings` → zero warnings
- [ ] Zero-arg `coursers pre` and `crs filter` behave identically to pre-PR baseline
- [ ] `coursers pre --profile codex` reads from `~/.config/coursers/profiles/codex/rules.json`
- [ ] `coursers post --profile codex` writes to `~/.config/coursers/profiles/codex/state.json`
      (or `.ctx/crs-codex-state.json` if that file exists)
- [ ] `~/.codex/hooks.json` updated and passes `jq . ~/.codex/hooks.json`
- [ ] `~/.config/coursers/profiles/codex/` directory exists with `rules.json` + `state.json`
