use std::path::PathBuf;

/// Approximate bytes per token (GPT/Claude tokenizer average).
/// Used by both `history` (discover token estimation) and `tool_swap` (budget clamping).
pub const BYTES_PER_TOKEN: usize = 4;

pub fn rules_path() -> PathBuf {
    if let Ok(p) = std::env::var("COURSERS_RULES") {
        return PathBuf::from(p);
    }
    dirs::home_dir()
        .expect("home dir")
        .join(".config/coursers/course-correct-rules.json")
}

pub fn state_path_default() -> PathBuf {
    if let Ok(p) = std::env::var("COURSERS_STATE") {
        return PathBuf::from(p);
    }
    let local = PathBuf::from(".ctx/course-correct-state.json");
    if local.exists() {
        return local;
    }
    dirs::home_dir()
        .expect("home dir")
        .join(".config/coursers/course-correct-state.json")
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
    use super::rules_path;
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
}
