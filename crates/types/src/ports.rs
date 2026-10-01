//! Port traits for the coursers hexagonal architecture.
//!
//! All traits use associated error types to keep this crate dependency-free.

pub mod rules {
    use crate::rules::RulesConfig;

    pub trait RulesLoader {
        type Error: std::fmt::Debug;
        /// Loads the effective blocking-rule configuration.
        fn load(&self) -> Result<RulesConfig, Self::Error>;
    }
}

pub mod state {
    use crate::state::State;

    pub trait StateStore {
        type Error: std::fmt::Debug;
        /// Loads the current failure-learning state.
        fn load(&self) -> Result<State, Self::Error>;
        /// Persists the supplied failure-learning state.
        fn save(&self, state: &State) -> Result<(), Self::Error>;
    }
}

pub mod capture {
    use crate::capture::SuggestionRecord;

    pub trait CaptureStore {
        type Error: std::fmt::Debug;
        /// Records a captured suggestion event.
        fn record(&self, record: SuggestionRecord) -> Result<(), Self::Error>;
        /// Marks matching pending suggestions as accepted by a later command.
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
        /// Returns the historical command records supplied by this source.
        fn commands(&self) -> impl Iterator<Item = CommandRecord>;
    }
}

pub mod stats {
    use crate::stats::Stats;

    pub trait StatsStore {
        type Error: std::fmt::Debug;
        /// Loads the current block statistics.
        fn load(&self) -> Result<Stats, Self::Error>;
        /// Persists the supplied block statistics.
        fn save(&self, stats: &Stats) -> Result<(), Self::Error>;
    }
}

pub mod filters {
    use crate::filters::FiltersConfig;
    use std::path::PathBuf;

    pub trait FiltersLoader {
        type Error: std::fmt::Debug;
        /// Loads the effective filter configuration.
        fn load(&self) -> Result<FiltersConfig, Self::Error>;
        /// Returns the source path of the effective filter configuration, if known.
        fn filters_path(&self) -> Option<PathBuf>;
    }
}

pub mod obfsck {
    use crate::obfsck::{AuditHit, FilterSuggestion};

    pub trait ObfsckMcp {
        /// Audits text and returns detected sensitive-data matches.
        fn audit(&self, text: &str) -> Vec<AuditHit>;
        /// Generates filter suggestions from representative examples.
        fn generate_filters(&self, examples: &[String]) -> Vec<FilterSuggestion>;
    }
}

pub mod rtk {
    use crate::rtk::*;

    pub trait RtkAnalysis {
        /// Returns RTK discovery results for the requested lookback period.
        fn discover(&self, since_days: u32) -> Option<RtkDiscoverReport>;
        /// Returns RTK token-savings totals when available.
        fn gain(&self) -> Option<RtkGainReport>;
        /// Returns RTK statistics for the current session when available.
        fn session(&self) -> Option<Vec<RtkSessionEntry>>;
        /// Returns RTK installation and hook verification results.
        fn verify(&self) -> Option<RtkVerifyResult>;
        /// Returns RTK hook-audit results when available.
        fn hook_audit(&self) -> Option<RtkHookAudit>;
        /// Returns the installed RTK version when available.
        fn version(&self) -> Option<String>;
    }

    pub trait RtkRewrite {
        /// Returns RTK's rewritten form of a command, if supported.
        fn rewrite(&self, command: &str) -> Option<String>;
        /// Returns RTK support and rewrite details for a command.
        fn probe(&self, command: &str) -> Option<RtkProbeResult>;
        /// Reports whether RTK supports the command.
        fn check(&self, command: &str) -> bool;
        /// Lists the command proxies managed by RTK.
        fn list_proxies(&self) -> Vec<String>;
        /// Flushes RTK's accumulated state and reports success.
        fn flush(&self) -> bool;
    }
}

pub mod expand {
    pub trait VarExpander {
        /// Expands supported variable references in a command string.
        fn expand(&self, command: &str) -> String;
    }
}

pub mod tool_swap {
    pub trait FileInfo {
        /// Returns the file size in bytes, if the path is readable.
        fn file_size(&self, path: &str) -> Option<u64>;
        /// Returns the number of lines in the file, if readable.
        fn count_lines(&self, path: &str) -> Option<usize>;
        /// Returns the file's average bytes per line, if measurable.
        fn avg_bytes_per_line(&self, path: &str) -> Option<usize>;
    }
}
