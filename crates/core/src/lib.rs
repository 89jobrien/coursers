// TODO(unreachable-pub): add `#![warn(unreachable_pub)]` once the public API
// surface is intentionally locked down. Currently many items are `pub` for
// test/integration use; audit and tighten to `pub(crate)` where appropriate.
pub mod error;
pub use error::CourserError;

pub mod analyze;
pub mod config;
pub mod date;
pub mod hook;
pub mod loader;
pub mod obfsck;
pub mod parse;
pub mod replay;
pub mod rtk;
pub mod rules;
pub mod rx_prefix;
pub mod state;
pub mod store;
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
pub use hook::filters;
pub use hook::pipeline as hook_pipeline;
pub use hook::rewrite;
pub use hook::tool_swap;
pub use parse::ast;
pub use parse::expand;
pub use parse::pipeline;
