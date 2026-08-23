//! HookChain-based pre/post execution path (opt-in via `COURSERS_HOOK_CHAIN=1`).
//!
//! This module provides [`run_pre`] and [`run_post`] entry points that replace the
//! legacy `hook::pre` / `hook::post` paths when the `COURSERS_HOOK_CHAIN` environment
//! variable is set to `1`.  The legacy paths remain the **default** — this path is
//! **not yet enabled in production**.  A follow-up commit will flip the switch after
//! validation that outcomes are equivalent.
//!
//! # Opt-in mechanism
//!
//! ```sh
//! COURSERS_HOOK_CHAIN=1 coursers pre   # chain path
//! coursers pre                         # legacy path (unchanged)
//! ```
//!
//! # Known gaps vs. legacy path
//!
//! - Signal exit codes (130, 137, 143) are **not** excluded from failure-learning
//!   in [`FailureObserver`].  The legacy `post.rs` has this exclusion; the chain
//!   path inherits whatever `FailureObserver` implements.  This is a documented
//!   gap to be closed before making the chain the default.
//! - The `ls`-enrichment and `record_correction` side-effects in `pre.rs` (eza/find
//!   tree in deny messages, hook-log writes) are **not** replicated here.  The chain
//!   deny message is the raw rule message without directory listing enrichment.
//! - The capture-store (fine-tuning dataset recording) in `pre.rs` and `post.rs` is
//!   **not** wired into the chain path.

use std::io::{self, Read, Write};

use coursers_core::config::ProfileConfig;
use coursers_core::hook::chain::{HookContext, PostHookOutcome, PreHookOutcome, ToolOutput};

// ---------------------------------------------------------------------------
// Activation gate
// ---------------------------------------------------------------------------

/// Returns `true` when `COURSERS_HOOK_CHAIN=1` is set in the environment.
///
/// This is the single source of truth for the opt-in check used in `lib.rs`.
pub fn chain_enabled() -> bool {
    std::env::var("COURSERS_HOOK_CHAIN").as_deref() == Ok("1")
}

// ---------------------------------------------------------------------------
// Stdin helpers
// ---------------------------------------------------------------------------

fn read_stdin_json() -> Option<serde_json::Value> {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf).ok()?;
    serde_json::from_str(&buf).ok()
}

fn bash_context(command: &str) -> HookContext {
    HookContext::new("Bash", serde_json::json!({ "command": command }))
}

// ---------------------------------------------------------------------------
// Pre-hook chain runner
// ---------------------------------------------------------------------------

/// Run the chain-based PreToolUse path.
///
/// Reads a Claude Code hook JSON payload from stdin, extracts the Bash command,
/// runs the assembled [`HookChain`], and translates the outcome to the Claude Code
/// protocol:
///
/// | Outcome        | stdout                        | exit |
/// |----------------|-------------------------------|------|
/// | `Allow`        | (silent)                      | 0    |
/// | `Deny(msg)`    | deny JSON                     | 2    |
/// | `Rewrite{..}`  | rewrite JSON                  | 0    |
// qual:allow(iosp) reason: "I/O boundary — reads stdin, writes stdout, may exit"
pub fn run_pre(profile_cfg: &ProfileConfig) {
    let Some(raw) = read_stdin_json() else {
        return;
    };

    // Only handle Bash tool calls.
    if raw.get("tool_name").and_then(|v| v.as_str()) != Some("Bash") {
        return;
    }

    let command = raw
        .get("tool_input")
        .and_then(|i| i.get("command"))
        .and_then(|c| c.as_str())
        .unwrap_or("");

    if command.is_empty() {
        return;
    }

    let ctx = bash_context(command);
    let chain = profile_cfg.build_hook_chain();

    let outcome = match chain.run_pre(&ctx) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("[crs chain] pre error: {e}");
            return;
        }
    };

    match outcome {
        PreHookOutcome::Allow => {}
        PreHookOutcome::Deny(msg) => {
            // No byte-range span available here — HookChain doesn't thread one
            // through from the underlying rule match, unlike the legacy
            // pre.rs path (see matched_span there).
            let rendered =
                coursers_core::diagnostics::RuleViolation::new(command, &msg, None).render();
            emit_deny(profile_cfg.protocol, &rendered);
        }
        PreHookOutcome::Rewrite { command, reason } => {
            emit_rewrite(&command, &reason);
        }
    }
}

// ---------------------------------------------------------------------------
// Post-hook chain runner
// ---------------------------------------------------------------------------

/// Run the chain-based PostToolUse path.
///
/// Reads a Claude Code hook JSON payload from stdin, extracts the command and
/// tool response, runs the assembled [`HookChain`], and translates the outcome:
///
/// | Outcome          | stdout                   | exit |
/// |------------------|--------------------------|------|
/// | `Allow`          | (silent)                 | 0    |
/// | `Filter(text)`   | filter-result JSON       | 0    |
// qual:allow(iosp) reason: "I/O boundary — reads stdin, writes stdout"
pub fn run_post(profile_cfg: &ProfileConfig) {
    let Some(raw) = read_stdin_json() else {
        return;
    };

    if raw.get("tool_name").and_then(|v| v.as_str()) != Some("Bash") {
        return;
    }

    let command = raw
        .get("tool_input")
        .and_then(|i| i.get("command"))
        .and_then(|c| c.as_str())
        .unwrap_or("");

    if command.is_empty() {
        return;
    }

    let exit_code = raw
        .get("tool_response")
        .and_then(|r| r.get("exit_code"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let output_text = raw
        .get("tool_response")
        .and_then(coursers_core::hook::protocol::extract_output)
        .unwrap_or_default();

    let ctx = bash_context(command);
    let tool_output = ToolOutput {
        text: output_text,
        exit_code,
    };

    let chain = profile_cfg.build_hook_chain();

    let outcome = match chain.run_post(&ctx, &tool_output) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("[crs chain] post error: {e}");
            return;
        }
    };

    match outcome {
        PostHookOutcome::Allow => {}
        PostHookOutcome::Filter(text) => {
            emit_filter(&text);
        }
    }
}

// ---------------------------------------------------------------------------
// Protocol helpers (private)
// ---------------------------------------------------------------------------

// qual:allow(iosp) reason: "I/O boundary — writes stdout, calls process::exit on deny"
fn emit_deny(proto: coursers_core::config::HookProtocol, reason: &str) {
    let (json, exit_code) = coursers_core::hook::protocol::deny_response(proto, reason);
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{json}").ok();
    handle.flush().ok();
    drop(handle);
    std::process::exit(exit_code);
}

fn emit_rewrite(command: &str, reason: &str) {
    let json = coursers_core::hook::protocol::rewrite_response(reason, command);
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{json}").ok();
    handle.flush().ok();
}

fn emit_filter(text: &str) {
    let json = coursers_core::hook::protocol::filter_result_response(text);
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{json}").ok();
    handle.flush().ok();
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Single shared mutex to serialize all env-mutating tests in this module.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn chain_enabled_false_when_unset() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("COURSERS_HOOK_CHAIN") };
        assert!(!chain_enabled());
    }

    #[test]
    fn chain_enabled_true_when_set_to_1() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("COURSERS_HOOK_CHAIN", "1") };
        let result = chain_enabled();
        unsafe { std::env::remove_var("COURSERS_HOOK_CHAIN") };
        assert!(result);
    }

    #[test]
    fn chain_enabled_false_for_other_values() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("COURSERS_HOOK_CHAIN", "true") };
        let result = chain_enabled();
        unsafe { std::env::remove_var("COURSERS_HOOK_CHAIN") };
        assert!(!result);
    }
}
