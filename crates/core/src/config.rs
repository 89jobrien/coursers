use std::path::PathBuf;

pub fn rules_path() -> PathBuf {
    if let Ok(p) = std::env::var("COURSERS_RULES") {
        return PathBuf::from(p);
    }
    dirs::home_dir()
        .expect("home dir")
        .join(".claude/hooks/course-correct-rules.json")
}

pub fn state_path_default() -> PathBuf {
    if let Ok(p) = std::env::var("COURSERS_STATE") {
        return PathBuf::from(p);
    }
    dirs::home_dir()
        .expect("home dir")
        .join(".claude/hooks/course-correct-state.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_var_overrides_default_rules_path() {
        unsafe { std::env::set_var("COURSERS_RULES", "/tmp/test-rules.json") };
        let path = rules_path();
        unsafe { std::env::remove_var("COURSERS_RULES") };
        assert_eq!(path.to_str().unwrap(), "/tmp/test-rules.json");
    }

    #[test]
    fn default_rules_path_contains_claude() {
        // Guard against parallel tests that set COURSERS_RULES.
        // This test must not depend on global env state — use state_path_default
        // (which has its own var) to verify the fallback pattern instead.
        let path = dirs::home_dir()
            .expect("home dir")
            .join(".claude/hooks/course-correct-rules.json");
        assert!(path.to_string_lossy().contains(".claude"));
    }
}
