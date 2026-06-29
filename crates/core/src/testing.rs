//! Test support: `MockWorkspace` — a composable builder for injecting templated and
//! dynamic test fixtures across crs-core tests.
//!
//! Gated behind `#[cfg(any(test, feature = "testing"))]` so it never ships in
//! production builds unless explicitly opted in.

use crate::history::{CommandRecord, CommandSource, DiscoverOpts};
use crate::rules::{FailureLearning, Rule, RulesConfig};
use crate::state::State;

/// Composable test fixture for crs-core domain functions.
///
/// Build with method chaining; call [`MockWorkspace::command_source`] to get a
/// `CommandSource` impl suitable for passing to `discover()` et al.
///
/// # Example
///
/// ```rust
/// # use coursers_core::testing::MockWorkspace;
/// let ws = MockWorkspace::new()
///     .with_command("grep foo .")
///     .with_command_ts("new cmd here", "2099-12-31T00:00:00Z")
///     .with_rule("no-grep", r"\bgrep\b")
///     .with_cwd("/project/a");
///
/// let src = ws.command_source();
/// let report = coursers_core::history::discover(&src, &ws.rules, &ws.discover_opts());
/// ```
// qual:allow(srp) reason: "builder pattern for test fixtures"
pub struct MockWorkspace {
    pub commands: Vec<CommandRecord>,
    pub rules: Vec<Rule>,
    pub failure_learning: FailureLearning,
    pub state: State,
    pub cwd: String,
}

impl Default for MockWorkspace {
    fn default() -> Self {
        Self {
            commands: vec![],
            rules: vec![],
            failure_learning: FailureLearning::default(),
            state: State::default(),
            cwd: "/project".to_string(),
        }
    }
}

impl MockWorkspace {
    /// Create an empty workspace with default cwd `/project`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a command in the workspace's default cwd with no timestamp.
    pub fn with_command(mut self, cmd: &str) -> Self {
        self.commands.push(CommandRecord {
            command: cmd.to_string(),
            session_id: "test-session".to_string(),
            cwd: self.cwd.clone(),
            timestamp: None,
            output_bytes: None,
        });
        self
    }

    /// Add a command with an explicit cwd.
    pub fn with_command_at(mut self, cmd: &str, cwd: &str) -> Self {
        self.commands.push(CommandRecord {
            command: cmd.to_string(),
            session_id: "test-session".to_string(),
            cwd: cwd.to_string(),
            timestamp: None,
            output_bytes: None,
        });
        self
    }

    /// Add a command with an ISO 8601 timestamp (e.g. `"2099-12-31T00:00:00Z"`).
    pub fn with_command_ts(mut self, cmd: &str, ts: &str) -> Self {
        self.commands.push(CommandRecord {
            command: cmd.to_string(),
            session_id: "test-session".to_string(),
            cwd: self.cwd.clone(),
            timestamp: Some(ts.to_string()),
            output_bytes: None,
        });
        self
    }

    /// Add a command with a known output byte count (used for token estimation).
    pub fn with_command_bytes(mut self, cmd: &str, output_bytes: usize) -> Self {
        self.commands.push(CommandRecord {
            command: cmd.to_string(),
            session_id: "test-session".to_string(),
            cwd: self.cwd.clone(),
            timestamp: None,
            output_bytes: Some(output_bytes),
        });
        self
    }

    /// Add a blocking rule with no exceptions.
    pub fn with_rule(mut self, id: &str, pattern: &str) -> Self {
        self.rules.push(Rule {
            id: id.to_string(),
            enabled: true,
            pattern: pattern.to_string(),
            pattern_flags: String::new(),
            exceptions: vec![],
            target_commands: vec![],
            message: None,
        });
        self
    }

    /// Add a blocking rule with a single exception pattern.
    pub fn with_rule_exception(mut self, id: &str, pattern: &str, exception: &str) -> Self {
        self.rules.push(Rule {
            id: id.to_string(),
            enabled: true,
            pattern: pattern.to_string(),
            pattern_flags: String::new(),
            exceptions: vec![exception.to_string()],
            target_commands: vec![],
            message: None,
        });
        self
    }

    /// Override the default cwd (`/project`). Affects subsequent `with_command` calls
    /// and the `DiscoverOpts` produced by [`discover_opts`].
    pub fn with_cwd(mut self, cwd: &str) -> Self {
        self.cwd = cwd.to_string();
        self
    }

    /// Override the failure learning config.
    pub fn with_failure_learning(mut self, fl: FailureLearning) -> Self {
        self.failure_learning = fl;
        self
    }

    /// Override the failure state (e.g. pre-populate with past failures).
    pub fn with_state(mut self, state: State) -> Self {
        self.state = state;
        self
    }

    /// Produce a `RulesConfig` combining [`rules`] and [`failure_learning`].
    pub fn rules_config(&self) -> RulesConfig {
        RulesConfig {
            rules: self.rules.clone(),
            failure_learning: self.failure_learning.clone(),
        }
    }

    /// Produce `DiscoverOpts` scoped to this workspace's cwd.
    pub fn discover_opts(&self) -> DiscoverOpts {
        DiscoverOpts {
            all_projects: false,
            current_dir: Some(std::path::PathBuf::from(&self.cwd)),
            ..Default::default()
        }
    }

    /// Produce a `DiscoverOpts` that spans all projects.
    pub fn discover_opts_all(&self) -> DiscoverOpts {
        DiscoverOpts {
            all_projects: true,
            ..Default::default()
        }
    }

    /// Produce a `MockCommandSource` from the accumulated commands.
    pub fn command_source(&self) -> MockCommandSource {
        MockCommandSource(
            self.commands
                .iter()
                .map(|r| CommandRecord {
                    command: r.command.clone(),
                    session_id: r.session_id.clone(),
                    cwd: r.cwd.clone(),
                    timestamp: r.timestamp.clone(),
                    output_bytes: r.output_bytes,
                })
                .collect(),
        )
    }
}

/// A `CommandSource` backed by an in-memory `Vec<CommandRecord>`.
pub struct MockCommandSource(pub Vec<CommandRecord>);

impl CommandSource for MockCommandSource {
    fn commands(&self) -> impl Iterator<Item = CommandRecord> {
        self.0.iter().map(|r| CommandRecord {
            command: r.command.clone(),
            session_id: r.session_id.clone(),
            cwd: r.cwd.clone(),
            timestamp: r.timestamp.clone(),
            output_bytes: r.output_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::discover;

    #[test]
    fn default_cwd_is_project() {
        let ws = MockWorkspace::new();
        assert_eq!(ws.cwd, "/project");
    }

    #[test]
    fn with_command_uses_default_cwd() {
        let ws = MockWorkspace::new().with_command("cargo build");
        assert_eq!(ws.commands[0].cwd, "/project");
        assert_eq!(ws.commands[0].session_id, "test-session");
    }

    #[test]
    fn with_command_at_overrides_cwd() {
        let ws = MockWorkspace::new().with_command_at("cargo build", "/other");
        assert_eq!(ws.commands[0].cwd, "/other");
    }

    #[test]
    fn with_command_ts_sets_timestamp() {
        let ws = MockWorkspace::new().with_command_ts("cargo build", "2099-01-01T00:00:00Z");
        assert_eq!(
            ws.commands[0].timestamp.as_deref(),
            Some("2099-01-01T00:00:00Z")
        );
    }

    #[test]
    fn with_command_bytes_sets_output_bytes() {
        let ws = MockWorkspace::new().with_command_bytes("cargo build", 4096);
        assert_eq!(ws.commands[0].output_bytes, Some(4096));
    }

    #[test]
    fn with_cwd_affects_subsequent_commands() {
        let ws = MockWorkspace::new()
            .with_cwd("/repo")
            .with_command("cargo test");
        assert_eq!(ws.commands[0].cwd, "/repo");
    }

    #[test]
    fn with_rule_adds_rule() {
        let ws = MockWorkspace::new().with_rule("no-grep", r"\bgrep\b");
        assert_eq!(ws.rules.len(), 1);
        assert_eq!(ws.rules[0].id, "no-grep");
        assert!(ws.rules[0].exceptions.is_empty());
    }

    #[test]
    fn with_rule_exception_adds_exception() {
        let ws = MockWorkspace::new().with_rule_exception("no-grep", r"\bgrep\b", r"\| grep");
        assert_eq!(ws.rules[0].exceptions.len(), 1);
    }

    #[test]
    fn command_source_iterates_all_commands() {
        let ws = MockWorkspace::new()
            .with_command("cargo build")
            .with_command("cargo test");
        let count = ws.command_source().commands().count();
        assert_eq!(count, 2);
    }

    #[test]
    fn discover_opts_scopes_to_cwd() {
        let ws = MockWorkspace::new().with_cwd("/myproject");
        let opts = ws.discover_opts();
        assert!(!opts.all_projects);
        assert_eq!(opts.current_dir.unwrap().to_str().unwrap(), "/myproject");
    }

    #[test]
    fn discover_opts_all_spans_all_projects() {
        let ws = MockWorkspace::new();
        assert!(ws.discover_opts_all().all_projects);
    }

    #[test]
    fn end_to_end_discover_with_mock_workspace() {
        // Use 2-token commands so both share the stem "grep" (3-token cmds get a 2-token stem).
        let ws = MockWorkspace::new()
            .with_command("grep .")
            .with_command("grep /tmp")
            .with_rule("no-grep", r"\bgrep\b");

        let src = ws.command_source();
        let report = discover(&src, &ws.rules, &ws.discover_opts_all());

        assert_eq!(report.scanned_commands, 2);
        assert_eq!(report.intercepted.len(), 1);
        assert_eq!(report.intercepted[0].stem, "grep");
        assert_eq!(report.intercepted[0].count, 2);
    }
}
