use crate::rules::{load as fs_load, RulesConfig};

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
