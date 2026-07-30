//! Port traits for the coursers hexagonal architecture.
//!
//! All traits use associated error types to keep this crate dependency-free.

pub mod rules {
    use crate::rules::RulesConfig;

    pub trait RulesLoader {
        type Error: std::fmt::Debug;
        fn load(&self) -> Result<RulesConfig, Self::Error>;
    }
}

pub mod state {
    use crate::state::State;

    pub trait StateStore {
        type Error: std::fmt::Debug;
        fn load(&self) -> Result<State, Self::Error>;
        fn save(&self, state: &State) -> Result<(), Self::Error>;
    }
}

pub mod capture {
    use crate::capture::SuggestionRecord;

    pub trait CaptureStore {
        type Error: std::fmt::Debug;
        fn record(&self, record: SuggestionRecord) -> Result<(), Self::Error>;
        fn mark_accepted(
            &self,
            session_id: &str,
            command: &str,
            exit_code: i64,
        ) -> Result<(), Self::Error>;
    }
}

pub mod history {
    use crate::history::CommandRecord;

    pub trait CommandSource {
        fn commands(&self) -> impl Iterator<Item = CommandRecord>;
    }
}

pub mod stats {
    use crate::stats::Stats;

    pub trait StatsStore {
        type Error: std::fmt::Debug;
        fn load(&self) -> Result<Stats, Self::Error>;
        fn save(&self, stats: &Stats) -> Result<(), Self::Error>;
    }
}

pub mod filters {
    use crate::filters::FiltersConfig;
    use std::path::PathBuf;

    pub trait FiltersLoader {
        type Error: std::fmt::Debug;
        fn load(&self) -> Result<FiltersConfig, Self::Error>;
        fn filters_path(&self) -> Option<PathBuf>;
    }
}

pub mod obfsck {
    use crate::obfsck::{AuditHit, FilterSuggestion};

    pub trait ObfsckMcp {
        fn audit(&self, text: &str) -> Vec<AuditHit>;
        fn generate_filters(&self, examples: &[String]) -> Vec<FilterSuggestion>;
    }
}

pub mod rtk {
    use crate::rtk::*;

    pub trait RtkAnalysis {
        fn discover(&self, since_days: u32) -> Option<RtkDiscoverReport>;
        fn gain(&self) -> Option<RtkGainReport>;
        fn session(&self) -> Option<Vec<RtkSessionEntry>>;
        fn verify(&self) -> Option<RtkVerifyResult>;
        fn hook_audit(&self) -> Option<RtkHookAudit>;
        fn version(&self) -> Option<String>;
    }

    pub trait RtkRewrite {
        fn rewrite(&self, command: &str) -> Option<String>;
        fn probe(&self, command: &str) -> Option<RtkProbeResult>;
        fn check(&self, command: &str) -> bool;
        fn list_proxies(&self) -> Vec<String>;
        fn flush(&self) -> bool;
    }
}

pub mod expand {
    pub trait VarExpander {
        fn expand(&self, command: &str) -> String;
    }
}

pub mod tool_swap {
    pub trait FileInfo {
        fn file_size(&self, path: &str) -> Option<u64>;
        fn count_lines(&self, path: &str) -> Option<usize>;
        fn avg_bytes_per_line(&self, path: &str) -> Option<usize>;
    }
}

#[cfg(test)]
mod tests {
    use super::capture::CaptureStore;
    use super::expand::VarExpander;
    use super::filters::FiltersLoader;
    use super::history::CommandSource;
    use super::rules::RulesLoader;
    use super::state::StateStore;
    use super::stats::StatsStore;
    use super::tool_swap::FileInfo;
    use crate::capture::SuggestionRecord;
    use crate::filters::FiltersConfig;
    use crate::history::CommandRecord;
    use crate::rules::RulesConfig;
    use crate::state::State;
    use crate::stats::Stats;
    use std::cell::RefCell;
    use std::path::PathBuf;

    // --- fakes exercising each trait boundary, per the hexagonal
    // dependency-injection convention (no filesystem access). ---

    struct FakeRulesLoader(RulesConfig);
    impl RulesLoader for FakeRulesLoader {
        type Error = std::io::Error;
        fn load(&self) -> Result<RulesConfig, Self::Error> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn rules_loader_fake_returns_configured_rules() {
        let loader = FakeRulesLoader(RulesConfig::default());
        let cfg = loader.load().unwrap();
        assert!(cfg.rules.is_empty());
    }

    struct FakeStateStore(RefCell<State>);
    impl StateStore for FakeStateStore {
        type Error = std::io::Error;
        fn load(&self) -> Result<State, Self::Error> {
            Ok(self.0.borrow().clone())
        }
        fn save(&self, state: &State) -> Result<(), Self::Error> {
            *self.0.borrow_mut() = state.clone();
            Ok(())
        }
    }

    #[test]
    fn state_store_fake_roundtrips_save_and_load() {
        let store = FakeStateStore(RefCell::new(State::default()));
        let mut state = State::default();
        state.failures.insert(
            "cmd".into(),
            crate::state::FailureEntry {
                command_preview: "cmd".into(),
                timestamps: vec![1],
                last_seen: 1.0,
            },
        );
        store.save(&state).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.failures.len(), 1);
    }

    struct FakeCaptureStore(RefCell<Vec<SuggestionRecord>>);
    impl CaptureStore for FakeCaptureStore {
        type Error = std::io::Error;
        fn record(&self, record: SuggestionRecord) -> Result<(), Self::Error> {
            self.0.borrow_mut().push(record);
            Ok(())
        }
        fn mark_accepted(
            &self,
            session_id: &str,
            command: &str,
            exit_code: i64,
        ) -> Result<(), Self::Error> {
            let mut records = self.0.borrow_mut();
            for r in records.iter_mut() {
                if r.session_id.as_deref() == Some(session_id) && r.original == command {
                    r.accepted = true;
                    r.exit_code = Some(exit_code);
                }
            }
            Ok(())
        }
    }

    #[test]
    fn capture_store_fake_records_and_marks_accepted() {
        let store = FakeCaptureStore(RefCell::new(Vec::new()));
        store
            .record(SuggestionRecord {
                ts: "t".into(),
                original: "cat foo".into(),
                suggestion: "bat foo".into(),
                rule_id: "r1".into(),
                cwd: "/tmp".into(),
                repo: None,
                session_id: Some("sess".into()),
                tool_name: "Bash".into(),
                accepted: false,
                accepted_ts: None,
                exit_code: None,
                count: 1,
            })
            .unwrap();
        store.mark_accepted("sess", "cat foo", 0).unwrap();
        let records = store.0.borrow();
        assert!(records[0].accepted);
        assert_eq!(records[0].exit_code, Some(0));
    }

    struct FakeCommandSource(Vec<CommandRecord>);
    impl CommandSource for FakeCommandSource {
        fn commands(&self) -> impl Iterator<Item = CommandRecord> {
            self.0.iter().map(|c| CommandRecord {
                command: c.command.clone(),
                session_id: c.session_id.clone(),
                cwd: c.cwd.clone(),
                timestamp: c.timestamp.clone(),
                output_bytes: c.output_bytes,
            })
        }
    }

    #[test]
    fn command_source_fake_yields_commands() {
        let source = FakeCommandSource(vec![CommandRecord {
            command: "ls".into(),
            session_id: "s1".into(),
            cwd: "/tmp".into(),
            timestamp: None,
            output_bytes: None,
        }]);
        let commands: Vec<_> = source.commands().collect();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command, "ls");
    }

    struct FakeStatsStore(RefCell<Stats>);
    impl StatsStore for FakeStatsStore {
        type Error = std::io::Error;
        fn load(&self) -> Result<Stats, Self::Error> {
            Ok(self.0.borrow().clone())
        }
        fn save(&self, stats: &Stats) -> Result<(), Self::Error> {
            *self.0.borrow_mut() = stats.clone();
            Ok(())
        }
    }

    #[test]
    fn stats_store_fake_roundtrips() {
        let store = FakeStatsStore(RefCell::new(Stats::default()));
        let mut stats = Stats::default();
        stats.blocks.insert("rule".into(), 3);
        store.save(&stats).unwrap();
        assert_eq!(store.load().unwrap().blocks["rule"], 3);
    }

    struct FakeFiltersLoader(FiltersConfig, Option<PathBuf>);
    impl FiltersLoader for FakeFiltersLoader {
        type Error = std::io::Error;
        fn load(&self) -> Result<FiltersConfig, Self::Error> {
            Ok(self.0.clone())
        }
        fn filters_path(&self) -> Option<PathBuf> {
            self.1.clone()
        }
    }

    #[test]
    fn filters_loader_fake_returns_path_and_config() {
        let loader = FakeFiltersLoader(FiltersConfig::default(), Some(PathBuf::from("/f.toml")));
        assert!(loader.load().unwrap().filters.is_empty());
        assert_eq!(loader.filters_path(), Some(PathBuf::from("/f.toml")));
    }

    struct FakeExpander;
    impl VarExpander for FakeExpander {
        fn expand(&self, command: &str) -> String {
            command.replace("$HOME", "/home/test")
        }
    }

    #[test]
    fn var_expander_fake_expands_placeholder() {
        let expander = FakeExpander;
        assert_eq!(expander.expand("cat $HOME/.env"), "cat /home/test/.env");
    }

    struct FakeFileInfo;
    impl FileInfo for FakeFileInfo {
        fn file_size(&self, path: &str) -> Option<u64> {
            if path == "known" { Some(1024) } else { None }
        }
        fn count_lines(&self, path: &str) -> Option<usize> {
            if path == "known" { Some(10) } else { None }
        }
        fn avg_bytes_per_line(&self, path: &str) -> Option<usize> {
            match (self.file_size(path), self.count_lines(path)) {
                (Some(size), Some(lines)) if lines > 0 => Some(size as usize / lines),
                _ => None,
            }
        }
    }

    #[test]
    fn file_info_fake_computes_avg_bytes_per_line() {
        let info = FakeFileInfo;
        assert_eq!(info.avg_bytes_per_line("known"), Some(102));
        assert_eq!(info.avg_bytes_per_line("missing"), None);
    }
}
