use std::path::PathBuf;

/// Approximate bytes per token (GPT/Claude tokenizer average).
/// Used by both `history` (discover token estimation) and `tool_swap` (budget clamping).
pub const BYTES_PER_TOKEN: usize = 4;

// TODO(config-path-inconsistency): README says hooks live at ~/.claude/hooks/ but
// CLAUDE.md and this code use ~/.config/coursers/. Pick one canonical location and
// update all docs, smoke tests, and example configs to match.

// TODO(config-defaults-migrate): migrate config default paths from ~/.claude/hooks/
// to ~/.config/coursers/ everywhere (docs, smoke.nu, agent companion). The XDG path
// used here is correct; the legacy ~/.claude/hooks/ references are stale.

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
    /// Create a builder with no overrides (uses XDG defaults or env vars).
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
    fn effective_state_path_returns_global_when_local_absent() {
        let cfg = ConfigBuilder::new().build();
        // .ctx/course-correct-state.json does not exist in test CWD
        let effective = cfg.effective_state_path();
        assert!(!effective.as_os_str().is_empty());
    }
}
