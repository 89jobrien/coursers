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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_opts_default_values() {
        let opts = DiscoverOpts::default();
        assert_eq!(opts.limit, 15);
        assert_eq!(opts.since_days, Some(30));
        assert!(!opts.all_projects);
        assert_eq!(opts.current_dir, None);
        assert_eq!(opts.min_count, 1);
    }

    #[test]
    fn command_record_holds_fields() {
        let rec = CommandRecord {
            command: "ls -la".into(),
            session_id: "sess-1".into(),
            cwd: "/tmp".into(),
            timestamp: Some("2026-01-01T00:00:00Z".into()),
            output_bytes: Some(128),
        };
        assert_eq!(rec.command, "ls -la");
        assert_eq!(rec.output_bytes, Some(128));
    }
}
