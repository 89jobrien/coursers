//! Analysis of captured suggestions, command history, insights, heatmaps, and statistics.

// TODO(adapter-crate-analysis-io): Move capture, stats, facet, and session-history
// filesystem implementations into coursers-adapters after the hook-path slice ships.
pub mod capture;
pub mod heat;
pub mod history;
pub mod insights;
pub mod stats;
pub mod suggest;
