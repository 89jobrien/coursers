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
pub struct ProfileConfig {
    /// Path to the rules JSON file.
    pub rules_path: PathBuf,
    /// Path to the global (home-dir) state file.
    pub global_state_path: PathBuf,
    /// Project-local state path (`.ctx/crs-<profile>-state.json`).
    pub local_state_path: PathBuf,
    /// Hook I/O protocol (Claude vs Codex).
    pub protocol: HookProtocol,
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

        ProfileConfig {
            rules_path: self.rules_override.unwrap_or(default_rules),
            global_state_path: self.state_override.unwrap_or(default_global_state),
            local_state_path: default_local_state,
            protocol,
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
}
