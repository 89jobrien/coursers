use std::path::PathBuf;

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
    fn default_rules_path_is_xdg() {
        // Ensure COURSERS_RULES is unset so we exercise the default branch.
        unsafe { std::env::remove_var("COURSERS_RULES") };
        let path = rules_path();
        assert!(
            path.to_string_lossy().contains(".config/coursers"),
            "expected XDG path, got: {}",
            path.display()
        );
    }
}
