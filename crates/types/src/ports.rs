//! Port traits for the coursers hexagonal architecture.
//!
//! All traits use associated error types to keep this crate dependency-free.
//!
//! # STATUS: UNUSED — this module has zero consumers
//!
//! As of 2026-09-29 no code path in the workspace imports `coursers_types::ports`
//! (verified: zero matches for `ports` across `crates/`). Every trait here is
//! independently redefined in `coursers-core` with a **different error contract**:
//! these use an associated `type Error: Debug`, the core copies take a concrete
//! `crate::error::CourserError`.
//!
//! # The domain types must move FIRST
//!
//! Migrating a trait in isolation does not work. Six of these traits are
//! infallible and have identical signatures to their core copies, but their
//! return types are currently **two distinct Rust types**, because ~15 domain
//! structs are also double-defined. Verified: 13 type names across 6 re-export
//! sites resolve to this crate — `rules::{FailureLearning, Rule, RulesConfig}`
//! (core/src/rules.rs:9), `state::{FailureEntry, State}` (core/src/state.rs:8),
//! `filters::{FilterMode, FilterRule, FiltersConfig}` (core/src/hook/filters.rs:6),
//! `filters::{FilterPayload, FilterResult}` (core/src/hook/filter_logic.rs:6),
//! `filters::{RewriteConfig, RewriteRule}` (core/src/hook/rewrite.rs:5), and
//! `filters::ToolSwapConfig` (core/src/hook/tool_swap.rs:8). In addition
//! `core/src/lib.rs:35` re-exports the whole crate as `pub use coursers_types as
//! types;`, which covers every remaining type. The following are NOT re-exported,
//! and are redefined in core:
//!
//!   - `obfsck::{AuditHit, FilterSuggestion}`  (core/src/obfsck.rs:13,20)
//!   - `rtk::{RtkDiscoverReport, RtkSupportedEntry, RtkUnsupportedEntry,
//!     RtkGainReport, RtkSessionEntry, RtkVerifyResult, RtkHookAudit,
//!     RtkAuditEntry, RtkProbeResult}`           (core/src/rtk.rs:13-96)
//!   - `stats::Stats`                            (core/src/analyze/stats.rs:14)
//!   - `history::{CommandRecord, DiscoverOpts}`  (core/src/analyze/history.rs:47,63)
//!   - `capture::SuggestionRecord`               (core/src/analyze/capture.rs:24)
//!
//! So `types::ports::obfsck::ObfsckMcp::audit` returns
//! `types::obfsck::AuditHit`, while `core::obfsck::ObfsckMcp::audit` returns
//! `core::obfsck::AuditHit`. Swapping the trait definition alone would break
//! every adapter (`NullObfsckMcpClient`, `ProcessObfsckMcpClient`,
//! `NullRtkClient`, `ProcessRtkClient`, `JsonlCommandSource`,
//! `MockCommandSource`, …). Correct order is data types -> traits.
//!
//! TODO(canonical-ports-migration): Step 1 — re-point the 15 duplicated domain
//! types in `coursers-core` at their `coursers_types` counterparts via
//! `pub use`, matching the pattern already used in `core/src/rules.rs:9` and
//! `core/src/state.rs:8`. This is additive and breaks nothing, since the
//! existing `core::` paths keep resolving. Watch for derive drift while doing
//! it: `types::stats::Stats` derives `Serialize, Deserialize, Default`, and
//! `types::history::DiscoverOpts` carries a `Default` impl that core's copy
//! defines separately. Step 2 — only then swap the 6 infallible traits
//! (`ObfsckMcp`, `RtkAnalysis`, `RtkRewrite`, `VarExpander`, `FileInfo`,
//! `CommandSource`) to re-export from here. Step 3 — the 5 fallible traits
//! (`RulesLoader`, `StateStore`, `CaptureStore`, `StatsStore`,
//! `FiltersLoader`) are additionally blocked on `CourserError`, which lives in
//! `coursers-core` and itself wraps `serde_json::Error`, `toml::de::Error`, and
//! `redb::*`, so it can never sit in a dependency-free ring. That needs a
//! decision between (a) letting this crate depend on serde/toml/redb, (b) a
//! neutral `PortError` here that core adapters map onto, or (c) leaving the
//! fallible ports in core and treating this module as infallible-only.
//! (`StatsStore` also needs `record_block`, a provided method core has and this
//! copy does not.)
//!
//! This file is registered as a hash-tracked protocol surface in `taskit.toml`,
//! so `check-protocol-drift` gates on a module nothing reads.

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
