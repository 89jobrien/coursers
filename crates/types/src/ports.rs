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
