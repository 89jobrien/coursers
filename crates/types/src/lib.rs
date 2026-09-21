//! Stable Coursers domain types and dependency-free port contracts.

// TODO(workspace-test-gaps): Add serialization tests for coursers-types, legacy/chain parity
// coverage, concurrent state-write tests, and a CI gate that compiles the fuzz workspace.
// TODO(ci-surface-gaps): Run the OpenCode Bun suite and installed-binary smoke/rule
// validation in CI alongside the Rust workspace gates.
pub mod capture;
pub mod config;
pub mod filters;
pub mod history;
pub mod hook;
pub mod obfsck;
pub mod pipeline;
pub mod ports;
pub mod rtk;
pub mod rules;
pub mod state;
pub mod stats;
