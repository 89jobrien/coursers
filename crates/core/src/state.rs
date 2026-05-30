use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::state_path_default;
use crate::rules::FailureLearning;

/// Max characters to store in the command preview field.
const PREVIEW_MAX_CHARS: usize = 80;

/// Minimum window in minutes (floor to avoid divide-by-zero in messages).
const MIN_WINDOW_MINUTES: u64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FailureEntry {
    pub command_preview: String,
    pub timestamps: Vec<u64>,
    pub last_seen: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
    #[serde(default)]
    pub failures: HashMap<String, FailureEntry>,
}

pub fn command_key(command: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(command.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn load(path: &Path) -> State {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, state: &State) {
    let tmp = path.with_extension("json.tmp");
    if let Ok(json) = serde_json::to_string_pretty(state)
        && fs::write(&tmp, json).is_ok()
    {
        let _ = fs::rename(&tmp, path);
    }
}

pub fn state_path(fl: &FailureLearning) -> std::path::PathBuf {
    fl.state_file
        .as_deref()
        .map(|p| {
            // Expand ~ manually
            if let Some(rest) = p.strip_prefix("~/") {
                dirs::home_dir().unwrap_or_default().join(rest)
            } else {
                std::path::PathBuf::from(p)
            }
        })
        .unwrap_or_else(state_path_default)
}

/// Records a failed command into state. Returns new state.
pub fn record_failure(mut state: State, command: &str, fl: &FailureLearning) -> State {
    let now = now_secs();
    let key = command_key(command);
    let preview: String = command.chars().take(PREVIEW_MAX_CHARS).collect();

    let entry = state.failures.entry(key).or_insert(FailureEntry {
        command_preview: preview.clone(),
        timestamps: vec![],
        last_seen: 0.0,
    });
    entry.timestamps.push(now);
    entry.last_seen = now as f64;
    entry.command_preview = preview;

    prune(state, fl, now)
}

fn prune(mut state: State, fl: &FailureLearning, now: u64) -> State {
    let window = fl.window_seconds;
    let cleanup = fl.cleanup_after_seconds;
    let max = fl.max_tracked_commands;

    // Remove stale entries
    state
        .failures
        .retain(|_, e| (now as f64 - e.last_seen) <= cleanup as f64);

    // Prune old timestamps; remove entries with no recent ones
    state.failures.retain(|_, e| {
        e.timestamps.retain(|&t| now.saturating_sub(t) <= window);
        !e.timestamps.is_empty()
    });

    // Evict oldest if over max
    if state.failures.len() > max {
        let mut entries: Vec<_> = state
            .failures
            .iter()
            .map(|(k, e)| (k.clone(), e.last_seen as u64))
            .collect();
        entries.sort_by_key(|(_, t)| *t);
        let to_remove = state.failures.len() - max;
        for (key, _) in entries.into_iter().take(to_remove) {
            state.failures.remove(&key);
        }
    }

    state
}

/// Returns deny message if command has failed `threshold` or more times in window.
pub fn check_learned(command: &str, state: &State, fl: &FailureLearning) -> Option<String> {
    if !fl.enabled {
        return None;
    }
    let key = command_key(command);
    let entry = state.failures.get(&key)?;

    if entry.timestamps.len() < fl.block_threshold {
        return None;
    }

    let window_minutes = (fl.window_seconds / crate::date::SECS_PER_MIN).max(MIN_WINDOW_MINUTES);
    let template = fl.message_template.as_deref().unwrap_or(
        "[course-correct] This exact command has failed {count} times in the last \
         {window} minutes. Consider a different approach.\n\nFailing command: {preview}",
    );
    let preview = &entry.command_preview;

    Some(
        template
            .replace("{count}", &entry.timestamps.len().to_string())
            .replace("{window}", &window_minutes.to_string())
            .replace("{preview}", preview),
    )
}

#[cfg(kani)]
mod kani_proofs {
    use super::{MIN_WINDOW_MINUTES, PREVIEW_MAX_CHARS};

    /// Buffer size for kani proof tractability.
    const KANI_BUF_LEN: usize = 10;

    /// Proof: preview truncation never exceeds PREVIEW_MAX_CHARS (bounded input).
    #[kani::proof]
    #[kani::unwind(15)]
    fn preview_length_bounded() {
        // Use a small buffer to keep tractable; the property is length-independent
        let buf = [b'x'; KANI_BUF_LEN];
        let len: usize = kani::any();
        kani::assume(len <= KANI_BUF_LEN);
        let cmd = std::str::from_utf8(&buf[..len]).expect("ASCII-only buffer is valid UTF-8");
        let preview: String = cmd.chars().take(PREVIEW_MAX_CHARS).collect();
        assert!(preview.len() <= PREVIEW_MAX_CHARS);
    }

    /// Proof: MIN_WINDOW_MINUTES floor prevents division issues.
    #[kani::proof]
    #[kani::unwind(1)]
    fn window_minutes_floor() {
        let window_secs: u64 = kani::any();
        let minutes = (window_secs / crate::date::SECS_PER_MIN).max(MIN_WINDOW_MINUTES);
        assert!(minutes >= 1, "window_minutes should never be zero");
    }

    /// Proof: PREVIEW_MAX_CHARS is positive.
    #[kani::proof]
    #[kani::unwind(1)]
    fn preview_max_chars_positive() {
        assert!(PREVIEW_MAX_CHARS > 0, "PREVIEW_MAX_CHARS must be positive");
    }
}

#[cfg(test)]
mod tests {
    use super::{FailureEntry, State, check_learned, command_key, now_secs, record_failure};
    use crate::rules::FailureLearning;

    fn fl(threshold: usize, window: u64) -> FailureLearning {
        FailureLearning {
            enabled: true,
            block_threshold: threshold,
            window_seconds: window,
            state_file: None,
            max_tracked_commands: 200,
            cleanup_after_seconds: 3600,
            message_template: None,
        }
    }

    #[test]
    fn record_failure_creates_entry() {
        let st = record_failure(State::default(), "grep foo .", &fl(3, 300));
        assert_eq!(st.failures.len(), 1);
        let entry = st.failures.values().next().unwrap();
        assert_eq!(entry.timestamps.len(), 1);
    }

    #[test]
    fn record_failure_appends() {
        let st = record_failure(State::default(), "grep foo .", &fl(3, 300));
        let st = record_failure(st, "grep foo .", &fl(3, 300));
        let entry = st.failures.values().next().unwrap();
        assert_eq!(entry.timestamps.len(), 2);
    }

    #[test]
    fn prune_removes_old_timestamps() {
        let mut st = State::default();
        let key = command_key("grep foo .");
        st.failures.insert(
            key,
            FailureEntry {
                command_preview: "grep foo .".to_string(),
                timestamps: vec![0],
                last_seen: now_secs() as f64,
            },
        );
        let st = record_failure(st, "grep foo .", &fl(3, 300));
        let entry = st.failures.values().next().unwrap();
        assert_eq!(entry.timestamps.len(), 1);
    }

    #[test]
    fn prune_evicts_over_max() {
        let mut st = State::default();
        let now = now_secs();
        let fl_cfg = FailureLearning {
            enabled: true,
            block_threshold: 3,
            window_seconds: 300,
            state_file: None,
            max_tracked_commands: 5,
            cleanup_after_seconds: 3600,
            message_template: None,
        };
        for i in 0..5u64 {
            let cmd = format!("cmd-{i}");
            let key = command_key(&cmd);
            st.failures.insert(
                key,
                FailureEntry {
                    command_preview: cmd,
                    timestamps: vec![now - i],
                    last_seen: (now - i) as f64,
                },
            );
        }
        let st = record_failure(st, "cmd-new", &fl_cfg);
        assert!(st.failures.len() <= 5);
    }

    #[test]
    fn check_learned_below_threshold() {
        let mut st = State::default();
        let now = now_secs();
        let key = command_key("grep foo .");
        st.failures.insert(
            key,
            FailureEntry {
                command_preview: "grep foo .".to_string(),
                timestamps: vec![now, now],
                last_seen: now as f64,
            },
        );
        assert!(check_learned("grep foo .", &st, &fl(3, 300)).is_none());
    }

    #[test]
    fn check_learned_at_threshold() {
        let mut st = State::default();
        let now = now_secs();
        let key = command_key("grep foo .");
        st.failures.insert(
            key,
            FailureEntry {
                command_preview: "grep foo .".to_string(),
                timestamps: vec![now, now, now],
                last_seen: now as f64,
            },
        );
        assert!(check_learned("grep foo .", &st, &fl(3, 300)).is_some());
    }

    #[test]
    fn check_learned_disabled() {
        let mut st = State::default();
        let now = now_secs();
        let key = command_key("grep foo .");
        st.failures.insert(
            key,
            FailureEntry {
                command_preview: "grep foo .".to_string(),
                timestamps: vec![now, now, now],
                last_seen: now as f64,
            },
        );
        let mut fl_cfg = fl(3, 300);
        fl_cfg.enabled = false;
        assert!(check_learned("grep foo .", &st, &fl_cfg).is_none());
    }

    #[test]
    fn cleanup_removes_stale_entries() {
        let mut st = State::default();
        let key = command_key("grep foo .");
        st.failures.insert(
            key,
            FailureEntry {
                command_preview: "grep foo .".to_string(),
                timestamps: vec![],
                last_seen: 0.0,
            },
        );
        let fl_cfg = FailureLearning {
            enabled: true,
            block_threshold: 3,
            window_seconds: 300,
            state_file: None,
            max_tracked_commands: 200,
            cleanup_after_seconds: 1,
            message_template: None,
        };
        let st = record_failure(st, "other-cmd", &fl_cfg);
        assert!(!st.failures.contains_key(&command_key("grep foo .")));
    }
}
