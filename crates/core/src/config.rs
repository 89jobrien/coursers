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
    dirs::home_dir()
        .expect("home dir")
        .join(".claude/hooks/course-correct-state.json")
}
