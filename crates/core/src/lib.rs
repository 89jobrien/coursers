//! Core rules, parsing, hook processing, state, and analysis for Coursers.

#![warn(unreachable_pub)]

// TODO(adapters-crate-consolidation): Six separate TODOs in this workspace all
// describe the same missing crate, `coursers-adapters`. Collect them here as a
// single migration board so the ordering is explicit rather than implied:
//
//   core/src/lib.rs          — adapter-crate-log-io, adapter-crate-session-io,
//                              adapter-crate-prefix-io
//   core/src/analyze/mod.rs  — adapter-crate-analysis-io
//   coursers/src/lib.rs      — adapter-crate-integration-io
//   coursers/src/crs_commands.rs — bounded-process-runner
//   types/src/filters.rs     — adapter-core-compat (deprecate the core loader)
//
// Blocked on the canonical-ports-migration marker in types/src/ports.rs: the
// adapter boundary is only meaningful once the ports sit in a ring above core.
// That marker also records that ~15 domain structs (obfsck, rtk, stats, history,
// capture) are double-defined and must be re-pointed at `coursers_types`
// *before* any trait can move, since the duplicate return types would otherwise
// break every adapter.
// Today `coursers-core` is not an inner ring at all — it does filesystem I/O,
// owns the `redb` hook log, calls `dirs::home_dir()`, and spawns `obfsck` and
// `git` as subprocesses.
//
// Recommended order: re-point the duplicated domain types in core, then land the
// six infallible port traits in types, then move log/session/prefix/analysis I/O
// out, then the subprocess clients, and only then delete the `deprecated` core
// aliases.

pub mod error;
pub use error::CourserError;

pub mod analyze;
pub mod config;
pub mod date;
pub mod diagnostics;
// TODO(adapter-crate-log-io): Move the redb-backed hook log into coursers-adapters
// after the initial hook execution adapters are stable.
pub mod hook;
// TODO(adapter-crate-session-io): Move JSONL session discovery and reading into
// coursers-adapters while keeping history aggregation in core.
pub mod jsonl_source;
pub mod loader;
pub mod obfsck;
pub mod parse;
pub mod replay;
pub mod rtk;
pub mod rules;
// TODO(adapter-crate-prefix-io): Move prefixe-backed file stores and path resolution
// behind published coursers-adapters constructors.
pub mod rx_prefix;
pub mod state;
pub mod store;
// TODO(core-package-conformance-tests): ensure package-scoped core test commands run every (#52)
// conformance and property suite without relying on workspace feature unification.
#[cfg(any(test, feature = "testing"))]
pub mod testing;

// Re-export the types crate for incremental migration.
pub use coursers_types as types;

// Re-exports for backward compatibility — external crates use `coursers_core::filters`, etc.
pub use analyze::capture;
pub use analyze::heat;
pub use analyze::history;
pub use analyze::insights;
pub use analyze::stats;
pub use analyze::suggest;
pub use hook::chain;
pub use hook::filters;
pub use hook::pipeline as hook_pipeline;
pub use hook::rewrite;
pub use hook::tool_swap;
pub use parse::ast;
pub use parse::expand;
pub use parse::pipeline;
