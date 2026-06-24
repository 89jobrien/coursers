# Design: Codex Profile Isolation

## Goal

Add profile-aware path resolution to `crs-core` so `coursers` and `crs` binaries can run
in Codex sessions with fully isolated rules, global state, and project-local state from
their Claude Code counterparts.

## Approved Approach

Profile-native `ConfigBuilder` in `crs-core` with layered resolution:
defaults → profile directory → CLI flags (flags always win).

## Crate Ownership

- **Owner crate**: `crs-core` (`crates/core`) — all path-resolution logic belongs here; binaries
  are thin CLI wrappers that forward profile/flag args into `ConfigBuilder`
- **Affected crates**: `crates/coursers` (pre/post hook binaries), `crates/crs` (filter/rewrite/
  discover/validate/probe), `~/.codex/hooks.json` (hook wiring config, not a crate)

## Public API

### Types

```rust
// crates/core/src/config.rs

/// Resolved paths for a named profile (or the default profile).
/// Constructed via `ConfigBuilder::build()`.
pub struct ProfileConfig {
    /// Path to the rules JSON file.
    pub rules_path: PathBuf,
    /// Path to the global (home-dir) state file.
    pub global_state_path: PathBuf,
    /// Project-local state path (`.ctx/crs-<profile>-state.json`).
    /// This path is returned by `effective_state_path()` when the file exists;
    /// otherwise falls back to `global_state_path`.
    pub local_state_path: PathBuf,
}

/// Builder for `ProfileConfig`. Layered resolution:
/// defaults → profile directory → explicit overrides.
pub struct ConfigBuilder {
    profile: Option<String>,
    rules_override: Option<PathBuf>,
    state_override: Option<PathBuf>,
}
```

### Functions

```rust
// crates/core/src/config.rs

impl ConfigBuilder {
    pub fn new() -> Self;

    /// Set a named profile. Resolves to `~/.config/coursers/profiles/<name>/`.
    pub fn profile(self, name: impl Into<String>) -> Self;

    /// Override the rules path, taking precedence over the profile directory.
    pub fn rules(self, path: PathBuf) -> Self;

    /// Override the global state path, taking precedence over the profile directory.
    pub fn state(self, path: PathBuf) -> Self;

    /// Consume the builder and produce a `ProfileConfig`.
    pub fn build(self) -> ProfileConfig;
}

impl ProfileConfig {
    /// Returns the project-local state path if it exists on disk,
    /// otherwise returns the global state path. This mirrors the existing
    /// `state_path_default()` local-wins logic, scoped to the profile.
    pub fn effective_state_path(&self) -> &PathBuf;
}

impl Default for ConfigBuilder {
    fn default() -> Self;
}
```

```rust
// crates/core/src/loader.rs

/// Loads rules from an explicit path (no env-var lookup).
/// Used when a `ProfileConfig` has resolved the path already.
pub struct ProfileFsRulesLoader {
    pub path: PathBuf,
}

impl RulesLoader for ProfileFsRulesLoader {
    fn load(&self) -> RulesConfig;
}
```

### Path resolution table

| Layer            | rules_path                                     | global_state_path                              | local_state_path                 |
| ---------------- | ---------------------------------------------- | ---------------------------------------------- | -------------------------------- |
| Default          | `~/.config/coursers/course-correct-rules.json` | `~/.config/coursers/course-correct-state.json` | `.ctx/course-correct-state.json` |
| `--profile foo`  | `~/.config/coursers/profiles/foo/rules.json`   | `~/.config/coursers/profiles/foo/state.json`   | `.ctx/crs-foo-state.json`        |
| `--rules <path>` | `<path>` (overrides profile)                   | (from profile or default)                      | (from profile or default)        |
| `--state <path>` | (from profile or default)                      | `<path>` (overrides profile)                   | (from profile or default)        |

## CLI Changes

### `coursers` binary

Both subcommands gain identical flags:

```
coursers pre  [--profile <name>] [--rules <path>] [--state <path>]
coursers post [--profile <name>] [--rules <path>] [--state <path>]
```

`hook_context()` in `crates/coursers/src/hook/mod.rs` is replaced by
`hook_context_with_profile(profile_cfg: &ProfileConfig)` which constructs
`ProfileFsRulesLoader` and `FsStateStore` from the resolved paths.
The existing zero-arg `run()` entry points remain; they call
`ConfigBuilder::new().build()` (default behavior unchanged).

### `crs` binary

All subcommands gain `--profile <name>`. Subcommands that read rules or state
additionally gain `--rules <path>` and/or `--state <path>`:

| Subcommand | `--profile` | `--rules` | `--state` |
| ---------- | ----------- | --------- | --------- |
| `filter`   | yes         | yes       | yes       |
| `rewrite`  | yes         | yes       | no        |
| `discover` | yes         | yes       | no        |
| `validate` | yes         | yes       | no        |
| `probe`    | yes         | yes       | no        |
| `stats`    | yes         | no        | no        |
| `insights` | no          | no        | no        |
| `audit`    | no          | no        | no        |
| `suggest`  | yes         | yes       | no        |

## Data Flow

1. **Source**: CLI args parsed by `clap` in each binary's `main.rs`
2. **Build**: `ConfigBuilder::new().profile(p).rules(r).state(s).build()` → `ProfileConfig`
3. **Loader**: `ProfileFsRulesLoader { path: profile_cfg.rules_path }` implements `RulesLoader`
4. **Store**: `FsStateStore { path: profile_cfg.effective_state_path().clone() }` (existing type,
   no changes needed — it already accepts an arbitrary `PathBuf`)
5. **Hook logic**: `run_with(&loader, &store, ...)` — unchanged, injected traits mean zero
   changes to the hook business logic in `pre.rs` / `post.rs`
6. **Sink**: deny/pass decision emitted to stdout as before

## Hexagonal Boundaries

- **Port** (trait): `RulesLoader` in `crs_core::loader` — unchanged
- **Port** (trait): `StateStore` in `crs_core::store` — unchanged
- **Adapter** (impl): `ProfileFsRulesLoader` in `crs_core::loader` — new, path-explicit variant
- **Adapter** (impl): `FsStateStore` in `crs_core::store` — existing, no changes needed
- **Config shell** (not a port): `ConfigBuilder` + `ProfileConfig` in `crs_core::config` —
  pure path arithmetic, no I/O, fully unit-testable without fakes

## Codex Hook Wiring

`~/.codex/hooks.json` additions (append to existing `PreToolUse`/`PostToolUse` arrays):

```json
"PreToolUse": [
  {
    "matcher": "Bash",
    "hooks": [
      { "type": "command", "command": "coursers pre --profile codex", "timeout": 15 },
      { "type": "command", "command": "crs rewrite --profile codex", "timeout": 15 }
    ]
  }
],
"PostToolUse": [
  {
    "matcher": "Bash",
    "hooks": [
      { "type": "command", "command": "coursers post --profile codex", "timeout": 15 },
      { "type": "command", "command": "crs filter --profile codex", "timeout": 15 }
    ]
  }
]
```

## Scaffold

New directory and files created as part of this work (not auto-generated; hand-maintained):

```
~/.config/coursers/profiles/codex/
  rules.json    — copy of base rules, Codex-specific tuning allowed
  state.json    — starts empty; written by coursers post --profile codex
```

## Out of Scope

- Changes to `~/.claude/settings.json` or Claude Code hook wiring
- Merging or sharing state between profiles
- New external crate dependencies
- Profile listing / management subcommand (`crs profiles list`) — future work
- Env-var override for profile name (e.g. `COURSERS_PROFILE`) — future work

## Risk

- [ ] Breaking API changes: **no** — `rules_path()` and `state_path_default()` remain public;
      `FsRulesLoader` unchanged; `FsStateStore` unchanged
- [ ] New external dependency: **no** — `dirs` already in `crs-core`
- [ ] Feature flag required: **no**
- [ ] Semver: patch bump on `crs-core`; minor bump on `coursers` and `crs` (new CLI flags,
      backward-compatible)
