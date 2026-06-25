use crate::rules::{RulesConfig, load as fs_load};

pub trait RulesLoader {
    fn load(&self) -> RulesConfig;
}

/// Loads rules from the filesystem (COURSERS_RULES env var or default path).
pub struct FsRulesLoader;

impl RulesLoader for FsRulesLoader {
    fn load(&self) -> RulesConfig {
        fs_load()
    }
}

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

/// In-memory loader for tests. Returns the config it was constructed with.
#[cfg(any(test, feature = "testing"))]
#[derive(Clone)]
pub struct InMemoryRulesLoader(pub RulesConfig);

#[cfg(any(test, feature = "testing"))]
impl RulesLoader for InMemoryRulesLoader {
    fn load(&self) -> RulesConfig {
        self.0.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{FailureLearning, Rule, RulesConfig};

    #[test]
    fn in_memory_loader_returns_its_config() {
        let rule = Rule {
            id: "test-rule".to_string(),
            enabled: true,
            pattern: r"\bgrep\b".to_string(),
            pattern_flags: String::new(),
            exceptions: vec![],
            target_commands: vec![],
            message: Some("Use Grep tool".to_string()),
        };
        let config = RulesConfig {
            rules: vec![rule],
            failure_learning: FailureLearning::default(),
        };
        let loader = InMemoryRulesLoader(config.clone());
        let loaded = loader.load();
        assert_eq!(loaded.rules.len(), 1);
        assert_eq!(loaded.rules[0].id, "test-rule");
        assert_eq!(loaded.rules[0].pattern, r"\bgrep\b");
    }

    #[test]
    fn in_memory_loader_empty_config() {
        let config = RulesConfig {
            rules: vec![],
            failure_learning: FailureLearning::default(),
        };
        let loader = InMemoryRulesLoader(config);
        let loaded = loader.load();
        assert!(loaded.rules.is_empty());
    }

    #[test]
    fn fs_loader_returns_default_on_missing_file() {
        // Set COURSERS_RULES to a nonexistent path
        unsafe { std::env::set_var("COURSERS_RULES", "/nonexistent/rules.json") };
        let loader = FsRulesLoader;
        let config = loader.load();
        unsafe { std::env::remove_var("COURSERS_RULES") };
        assert!(config.rules.is_empty());
    }

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
        writeln!(
            f,
            r#"{{"rules":[],"failure_learning":{{"enabled":false}}}}"#
        )
        .unwrap();
        let loader = ProfileFsRulesLoader {
            path: f.path().to_path_buf(),
        };
        let config = loader.load();
        assert!(!config.failure_learning.enabled);
    }

    /// Conformance test: malformed rules.json must return empty config, never panic.
    #[test]
    fn fs_rules_loader_returns_default_on_malformed_json() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "{{invalid json!!").unwrap();
        let loader = ProfileFsRulesLoader {
            path: f.path().to_path_buf(),
        };
        let config = loader.load();
        assert!(
            config.rules.is_empty(),
            "malformed rules.json must yield an empty rules vec"
        );
    }
}
