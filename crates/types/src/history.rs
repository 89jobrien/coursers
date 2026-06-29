use std::path::PathBuf;

/// A single Bash command extracted from a Claude Code session file.
pub struct CommandRecord {
    pub command: String,
    pub session_id: String,
    pub cwd: String,
    pub timestamp: Option<String>,
    pub output_bytes: Option<usize>,
}

/// Options controlling how `discover` filters and paginates command history.
pub struct DiscoverOpts {
    pub limit: usize,
    pub since_days: Option<u32>,
    pub all_projects: bool,
    pub current_dir: Option<PathBuf>,
    pub min_count: u64,
}

impl Default for DiscoverOpts {
    fn default() -> Self {
        Self {
            limit: 15,
            since_days: Some(30),
            all_projects: false,
            current_dir: None,
            min_count: 1,
        }
    }
}
