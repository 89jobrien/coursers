use std::path::{Path, PathBuf};

use crate::config::state_path_default;
use crate::error::CourserError;
use crate::rules::FailureLearning;
use crate::state::State;

pub trait StateStore {
    fn load(&self) -> Result<State, CourserError>;
    fn save(&self, state: &State) -> Result<(), CourserError>;
}

/// Resolve the state file path from `FailureLearning` config.
pub fn state_path(fl: &FailureLearning) -> PathBuf {
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

/// Reads/writes state JSON to a real file path.
pub struct FsStateStore {
    pub path: PathBuf,
}

impl FsStateStore {
    /// Load state from a file path. Returns `Err` on I/O or JSON failure.
    fn load_from(path: &Path) -> Result<State, CourserError> {
        let content = std::fs::read_to_string(path).map_err(CourserError::Io)?;
        serde_json::from_str(&content).map_err(CourserError::Json)
    }

    /// Atomically save state to a file path via tmp+rename.
    fn save_to(path: &Path, state: &State) -> Result<(), CourserError> {
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(state).map_err(CourserError::Json)?;
        std::fs::write(&tmp, json).map_err(CourserError::Io)?;
        std::fs::rename(&tmp, path).map_err(CourserError::Io)
    }
}

impl StateStore for FsStateStore {
    fn load(&self) -> Result<State, CourserError> {
        Self::load_from(&self.path)
    }

    fn save(&self, state: &State) -> Result<(), CourserError> {
        Self::save_to(&self.path, state)
    }
}

/// In-memory store for tests. No filesystem I/O.
#[cfg(any(test, feature = "testing"))]
pub struct InMemoryStateStore {
    inner: std::cell::RefCell<State>,
}

#[cfg(any(test, feature = "testing"))]
impl InMemoryStateStore {
    pub fn new() -> Self {
        Self {
            inner: std::cell::RefCell::new(State::default()),
        }
    }

    pub fn with_state(state: State) -> Self {
        Self {
            inner: std::cell::RefCell::new(state),
        }
    }

    pub fn get_state(&self) -> State {
        self.inner.borrow().clone()
    }
}

#[cfg(any(test, feature = "testing"))]
impl Default for InMemoryStateStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "testing"))]
impl StateStore for InMemoryStateStore {
    fn load(&self) -> Result<State, CourserError> {
        Ok(self.inner.borrow().clone())
    }

    fn save(&self, state: &State) -> Result<(), CourserError> {
        *self.inner.borrow_mut() = state.clone();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO(raii-env-guards): env mutation in tests (set_var/remove_var) uses
    // serialization locks (ENV_LOCK) rather than RAII isolation. Refactor to use
    // `temp_env::with_var` for cleaner, panic-safe env isolation in all test files.

    /// Conformance test: malformed state JSON must return Err, never panic.
    #[test]
    fn fs_state_store_returns_err_on_malformed_json() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(b"{bad: json").unwrap();
        let store = FsStateStore {
            path: f.path().to_path_buf(),
        };
        let result = store.load();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CourserError::Json(_)));
    }

    #[test]
    fn in_memory_store_roundtrip() {
        let store = InMemoryStateStore::new();
        let mut state = store.load().unwrap();
        assert!(state.failures.is_empty());

        state.failures.insert(
            "key".to_string(),
            crate::state::FailureEntry {
                command_preview: "echo hi".to_string(),
                timestamps: vec![1],
                last_seen: 1.0,
            },
        );
        store.save(&state).unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(loaded.failures.len(), 1);
        assert!(loaded.failures.contains_key("key"));
    }
}
