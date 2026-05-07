use std::path::{Path, PathBuf};

use crate::state::{State, load as fs_load, save as fs_save};

pub trait StateStore {
    fn load(&self) -> State;
    fn save(&self, state: &State);
    fn path(&self) -> &Path;
}

/// Reads/writes state JSON to a real file path.
pub struct FsStateStore {
    pub path: PathBuf,
}

impl StateStore for FsStateStore {
    fn load(&self) -> State {
        fs_load(&self.path)
    }

    fn save(&self, state: &State) {
        fs_save(&self.path, state);
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

/// In-memory store for tests. No filesystem I/O.
#[cfg(any(test, feature = "testing"))]
pub struct InMemoryStateStore {
    inner: std::cell::RefCell<State>,
    path: PathBuf,
}

#[cfg(any(test, feature = "testing"))]
impl InMemoryStateStore {
    pub fn new() -> Self {
        Self {
            inner: std::cell::RefCell::new(State::default()),
            path: PathBuf::from("/tmp/in-memory-state.json"),
        }
    }

    pub fn with_state(state: State) -> Self {
        Self {
            inner: std::cell::RefCell::new(state),
            path: PathBuf::from("/tmp/in-memory-state.json"),
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
    fn load(&self) -> State {
        self.inner.borrow().clone()
    }

    fn save(&self, state: &State) {
        *self.inner.borrow_mut() = state.clone();
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_store_roundtrip() {
        let store = InMemoryStateStore::new();
        let mut state = store.load();
        assert!(state.failures.is_empty());

        state.failures.insert(
            "key".to_string(),
            crate::state::FailureEntry {
                command_preview: "echo hi".to_string(),
                timestamps: vec![1],
                last_seen: 1.0,
            },
        );
        store.save(&state);

        let loaded = store.load();
        assert_eq!(loaded.failures.len(), 1);
        assert!(loaded.failures.contains_key("key"));
    }
}
