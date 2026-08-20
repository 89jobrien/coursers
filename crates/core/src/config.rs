use std::path::PathBuf;

/// Approximate bytes per token (GPT/Claude tokenizer average).
/// Used by both `history` (discover token estimation) and `tool_swap` (budget clamping).
pub const BYTES_PER_TOKEN: usize = 4;

// Canonical config location is XDG: ~/.config/coursers/. The only intentional
// ~/.claude/hooks/ reference left is `nu-check`'s scripts dir
// (~/.claude/hooks/nu), which holds live hook scripts, not config.

/// Resolve the rules config path: `COURSERS_RULES` env var or XDG default.
pub fn rules_path() -> PathBuf {
    if let Ok(p) = std::env::var("COURSERS_RULES") {
        return PathBuf::from(p);
    }
    dirs::home_dir()
        .unwrap_or_else(|| {
            eprintln!("[coursers] warning: could not resolve home directory; falling back to /tmp");
            std::path::PathBuf::from("/tmp")
        })
        .join(".config/coursers/course-correct-rules.json")
}

/// Resolve the state file path: project-local `.ctx/` wins over XDG global.
pub fn state_path_default() -> PathBuf {
    if let Ok(p) = std::env::var("COURSERS_STATE") {
        return PathBuf::from(p);
    }
    let local = PathBuf::from(".ctx/course-correct-state.json");
    if local.exists() {
        return local;
    }
    dirs::home_dir()
        .unwrap_or_else(|| {
            eprintln!("[coursers] warning: could not resolve home directory; falling back to /tmp");
            std::path::PathBuf::from("/tmp")
        })
        .join(".config/coursers/course-correct-state.json")
}

/// Resolve the state file path from `FailureLearning` config.
///
/// Handles `~/` prefix expansion and falls back to [`state_path_default`].
pub fn state_path(fl: &crate::rules::FailureLearning) -> std::path::PathBuf {
    fl.state_file
        .as_deref()
        .map(|p| {
            if let Some(rest) = p.strip_prefix("~/") {
                dirs::home_dir().unwrap_or_default().join(rest)
            } else {
                PathBuf::from(p)
            }
        })
        .unwrap_or_else(state_path_default)
}

#[derive(serde::Deserialize, Default)]
struct GodmodeStatus {
    #[serde(default)]
    running: Vec<String>,
}

/// Read running godmode task titles from `~/.cache/godmode/status.json`, a
/// file godmode writes on every status change. Used by the pre-hook to check
/// `Rule::task_override` glob matches without shelling out.
///
/// Missing or malformed cache file returns an empty vec — never errors. A
/// single small JSON read stays well under the pre-hook's 5ms budget.
pub fn running_task_titles() -> Vec<String> {
    let path = dirs::home_dir()
        .unwrap_or_default()
        .join(".cache/godmode/status.json");
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str::<GodmodeStatus>(&raw)
        .map(|s| s.running)
        .unwrap_or_default()
}

/// Which hook protocol to use for output formatting and exit codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HookProtocol {
    /// Claude Code: exit 2 for deny.
    #[default]
    Claude,
    /// Codex: exit 0 + JSON `permissionDecision: "deny"`.
    Codex,
}

/// Resolved paths for a named profile (or the default profile).
/// Constructed via [`ConfigBuilder::build`].
///
/// Starting with hc-c this struct is the **unified app configuration** —
/// all three config domains (rules, state, filters/rewrites) are resolved once
/// here so the rest of the codebase never has to rediscover paths.
///
/// Use the factory methods ([`rules_loader`][ProfileConfig::rules_loader],
/// [`state_store`][ProfileConfig::state_store],
/// [`filters_loader`][ProfileConfig::filters_loader],
/// [`rewrite_loader`][ProfileConfig::rewrite_loader]) to obtain ready-to-use
/// port-trait adapters for wiring a [`crate::hook::chain::HookChain`].
pub struct ProfileConfig {
    /// Path to the rules JSON file.
    pub rules_path: PathBuf,
    /// Path to the global (home-dir) state file.
    pub global_state_path: PathBuf,
    /// Project-local state path (`.ctx/crs-<profile>-state.json`).
    pub local_state_path: PathBuf,
    /// Hook I/O protocol (Claude vs Codex).
    pub protocol: HookProtocol,

    // ── Filters / rewrites ────────────────────────────────────────────────
    // Paths captured at build time so the hook implementations never re-probe
    // env vars or the filesystem.  All three are `Option` because any of them
    // may be absent (env var not set, files not on disk).
    /// Path from the `CRS_FILTERS` env var at the time this config was built.
    /// When present it is an exclusive override — project and global paths are
    /// ignored by the filter and rewrite loaders.
    pub filters_env_path: Option<PathBuf>,
    /// Project-local `.ctx/crs-filters.toml`, found by walking up from CWD at
    /// build time.  `None` if no such file was found.
    pub filters_project_path: Option<PathBuf>,
    /// Global `~/.config/crs/filters.toml` at build time.  `None` if absent.
    pub filters_global_path: Option<PathBuf>,
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

    // ── hc-c: factory methods for concrete port-trait adapters ───────────

    /// Return a rules loader that reads from the resolved `rules_path`.
    ///
    /// Suitable for use with [`crate::hook::concrete::RuleBlockHook`] and
    /// [`crate::hook::concrete::FailureObserver`].
    pub fn rules_loader(&self) -> crate::loader::ProfileFsRulesLoader {
        crate::loader::ProfileFsRulesLoader {
            path: self.rules_path.clone(),
        }
    }

    /// Return a state store that reads/writes the *effective* state path
    /// (project-local wins over global when the local file exists).
    ///
    /// Suitable for use with [`crate::hook::concrete::RuleBlockHook`] and
    /// [`crate::hook::concrete::FailureObserver`].
    pub fn state_store(&self) -> crate::store::FsStateStore {
        crate::store::FsStateStore {
            path: self.effective_state_path().clone(),
        }
    }

    /// Return a filters loader that uses the captured filter paths.
    ///
    /// Priority: `CRS_FILTERS` env override → project-local → global.
    /// Suitable for use with [`crate::hook::concrete::FilterHook`].
    pub fn filters_loader(&self) -> crate::hook::filters::ProfileFsFiltersLoader {
        crate::hook::filters::ProfileFsFiltersLoader {
            env_path: self.filters_env_path.clone(),
            project_path: self.filters_project_path.clone(),
            global_path: self.filters_global_path.clone(),
        }
    }

    /// Return a rewrite loader that uses the captured filter paths.
    ///
    /// Mirrors [`crate::hook::rewrite::FsRewriteLoader`] merge semantics:
    /// env override is exclusive; otherwise project + global are merged so that
    /// project rewrites evaluate first (first-match-wins).
    /// Suitable for use with [`crate::hook::concrete::RewriteHook`].
    pub fn rewrite_loader(&self) -> crate::hook::rewrite::ProfileFsRewriteLoader {
        crate::hook::rewrite::ProfileFsRewriteLoader {
            env_path: self.filters_env_path.clone(),
            project_path: self.filters_project_path.clone(),
            global_path: self.filters_global_path.clone(),
        }
    }
}

/// Builder for [`ProfileConfig`]. Layered resolution:
/// defaults → profile directory → explicit overrides.
pub struct ConfigBuilder {
    profile: Option<String>,
    rules_override: Option<PathBuf>,
    state_override: Option<PathBuf>,
    protocol_override: Option<HookProtocol>,
}

impl ConfigBuilder {
    /// Create a builder with no overrides (uses XDG defaults or env vars).
    pub fn new() -> Self {
        Self {
            profile: None,
            rules_override: None,
            state_override: None,
            protocol_override: None,
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

    /// Override the hook protocol; takes precedence over profile-name inference.
    pub fn protocol(mut self, proto: HookProtocol) -> Self {
        self.protocol_override = Some(proto);
        self
    }

    /// Resolve all paths and return a [`ProfileConfig`].
    pub fn build(self) -> ProfileConfig {
        let home = dirs::home_dir().unwrap_or_else(|| {
            eprintln!("[coursers] warning: could not resolve home directory; falling back to /tmp");
            std::path::PathBuf::from("/tmp")
        });
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
                // Respect legacy env-var overrides when no profile is set.
                let rules = if let Ok(p) = std::env::var("COURSERS_RULES") {
                    PathBuf::from(p)
                } else {
                    base.join("course-correct-rules.json")
                };
                let global_state = if let Ok(p) = std::env::var("COURSERS_STATE") {
                    PathBuf::from(p)
                } else {
                    base.join("course-correct-state.json")
                };
                (
                    rules,
                    global_state,
                    PathBuf::from(".ctx/course-correct-state.json"),
                )
            };

        let protocol = match self.protocol_override {
            Some(p) => p,
            None => match self.profile.as_deref() {
                Some("codex") => HookProtocol::Codex,
                _ => HookProtocol::Claude,
            },
        };

        // ── Filters / rewrites path resolution ───────────────────────────
        // Same three-level hierarchy as `hook::filters::{project_filters_path,
        // global_filters_path}` but captured once at build time.
        let filters_env_path = std::env::var("CRS_FILTERS").ok().map(PathBuf::from);
        let filters_project_path = crate::hook::filters::project_filters_path();
        let filters_global_path = crate::hook::filters::global_filters_path();

        ProfileConfig {
            rules_path: self.rules_override.unwrap_or(default_rules),
            global_state_path: self.state_override.unwrap_or(default_global_state),
            local_state_path: default_local_state,
            protocol,
            filters_env_path,
            filters_project_path,
            filters_global_path,
        }
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(kani)]
mod kani_proofs {
    use super::BYTES_PER_TOKEN;

    /// Proof: BYTES_PER_TOKEN is positive (used as divisor in token estimation).
    #[kani::proof]
    #[kani::unwind(1)]
    fn bytes_per_token_positive() {
        assert!(BYTES_PER_TOKEN > 0, "BYTES_PER_TOKEN must be positive");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize env-mutation tests to avoid races between parallel test threads.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn env_var_overrides_default_rules_path() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("COURSERS_RULES", "/tmp/test-rules.json") };
        let path = rules_path();
        unsafe { std::env::remove_var("COURSERS_RULES") };
        assert_eq!(path.to_str().unwrap(), "/tmp/test-rules.json");
    }

    #[test]
    fn default_rules_path_is_xdg() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("COURSERS_RULES") };
        let path = rules_path();
        assert!(
            path.to_string_lossy().contains(".config/coursers"),
            "expected XDG path, got: {}",
            path.display()
        );
    }

    // ── ConfigBuilder / ProfileConfig ─────────────────────────────────────

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
        assert_eq!(
            cfg.rules_path,
            std::path::PathBuf::from("/tmp/custom-rules.json")
        );
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
    fn default_builder_gives_claude_protocol() {
        let cfg = ConfigBuilder::new().build();
        assert_eq!(cfg.protocol, HookProtocol::Claude);
    }

    #[test]
    fn codex_profile_infers_codex_protocol() {
        let cfg = ConfigBuilder::new().profile("codex").build();
        assert_eq!(cfg.protocol, HookProtocol::Codex);
    }

    #[test]
    fn protocol_override_wins() {
        let cfg = ConfigBuilder::new()
            .profile("codex")
            .protocol(HookProtocol::Claude)
            .build();
        assert_eq!(cfg.protocol, HookProtocol::Claude);
    }

    // ── running_task_titles ────────────────────────────────────────────

    #[test]
    fn running_task_titles_parses_status_json() {
        let json = r#"{"running":["[t1] migrate stuff","[t2] add tests"],"pending":3}"#;
        let status: GodmodeStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status.running, vec!["[t1] migrate stuff", "[t2] add tests"]);
    }

    #[test]
    fn running_task_titles_defaults_empty_when_field_missing() {
        let status: GodmodeStatus = serde_json::from_str("{}").unwrap();
        assert!(status.running.is_empty());
    }

    #[test]
    fn running_task_titles_missing_cache_file_returns_empty() {
        // No env override for the cache path exists yet, so this exercises
        // the real ~/.cache/godmode/status.json — either absent (empty vec)
        // or present (some vec). Just assert it never panics.
        let _ = running_task_titles();
    }

    #[test]
    fn effective_state_path_returns_global_when_local_absent() {
        let cfg = ConfigBuilder::new().build();
        // .ctx/course-correct-state.json does not exist in test CWD
        let effective = cfg.effective_state_path();
        assert!(!effective.as_os_str().is_empty());
    }

    // ── hc-c: filters path resolution ─────────────────────────────────────

    #[test]
    fn filters_env_path_captured_when_set() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("CRS_FILTERS", "/tmp/my-filters.toml") };
        let cfg = ConfigBuilder::new().build();
        unsafe { std::env::remove_var("CRS_FILTERS") };
        assert_eq!(
            cfg.filters_env_path,
            Some(PathBuf::from("/tmp/my-filters.toml"))
        );
    }

    #[test]
    fn filters_env_path_none_when_not_set() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("CRS_FILTERS") };
        let cfg = ConfigBuilder::new().build();
        // env var not set → env_path must be None (project/global may or may
        // not be set depending on test CWD, so we only assert the env path)
        assert!(cfg.filters_env_path.is_none());
    }

    #[test]
    fn filters_env_path_wins_in_filters_loader() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("CRS_FILTERS", "/tmp/env-override-filters.toml") };
        let cfg = ConfigBuilder::new().build();
        unsafe { std::env::remove_var("CRS_FILTERS") };

        use crate::hook::filters::FiltersLoader as _;
        let loader = cfg.filters_loader();
        // The loader should report the env path as `filters_path()`.
        assert_eq!(
            loader.filters_path(),
            Some(PathBuf::from("/tmp/env-override-filters.toml"))
        );
    }

    #[test]
    fn filters_env_path_wins_in_rewrite_loader() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("CRS_FILTERS", "/tmp/env-override-filters.toml") };
        let cfg = ConfigBuilder::new().build();
        unsafe { std::env::remove_var("CRS_FILTERS") };

        let loader = cfg.rewrite_loader();
        assert_eq!(
            loader.env_path,
            Some(PathBuf::from("/tmp/env-override-filters.toml"))
        );
    }

    #[test]
    fn filters_loader_project_path_wins_over_global_when_no_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("CRS_FILTERS") };

        use crate::hook::filters::FiltersLoader as _;
        let loader = crate::hook::filters::ProfileFsFiltersLoader {
            env_path: None,
            project_path: Some(PathBuf::from("/tmp/project-filters.toml")),
            global_path: Some(PathBuf::from("/tmp/global-filters.toml")),
        };
        // filters_path() priority: project > global when env absent.
        assert_eq!(
            loader.filters_path(),
            Some(PathBuf::from("/tmp/project-filters.toml"))
        );
    }

    #[test]
    fn filters_loader_falls_back_to_global_when_project_absent() {
        let _guard = ENV_LOCK.lock().unwrap();
        use crate::hook::filters::FiltersLoader as _;
        let loader = crate::hook::filters::ProfileFsFiltersLoader {
            env_path: None,
            project_path: None,
            global_path: Some(PathBuf::from("/tmp/global-filters.toml")),
        };
        assert_eq!(
            loader.filters_path(),
            Some(PathBuf::from("/tmp/global-filters.toml"))
        );
    }

    #[test]
    fn filters_loader_returns_none_when_all_absent() {
        use crate::hook::filters::FiltersLoader as _;
        let loader = crate::hook::filters::ProfileFsFiltersLoader {
            env_path: None,
            project_path: None,
            global_path: None,
        };
        assert!(loader.filters_path().is_none());
    }

    // ── hc-c: factory method round-trips ──────────────────────────────────

    #[test]
    fn rules_loader_uses_resolved_rules_path() {
        let cfg = ConfigBuilder::new()
            .rules(PathBuf::from("/tmp/custom-rules.json"))
            .build();
        let loader = cfg.rules_loader();
        assert_eq!(loader.path, PathBuf::from("/tmp/custom-rules.json"));
    }

    #[test]
    fn state_store_uses_effective_state_path() {
        let cfg = ConfigBuilder::new()
            .state(PathBuf::from("/tmp/custom-state.json"))
            .build();
        let store = cfg.state_store();
        // When local_state_path does not exist, effective = global_state_path.
        // Here we overrode global to /tmp/custom-state.json so that's what we get.
        assert_eq!(store.path, PathBuf::from("/tmp/custom-state.json"));
    }
}
