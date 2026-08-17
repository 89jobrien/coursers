//! Port traits for composable hook chains.
//!
//! These traits define the hexagonal boundary between the hook-pipeline domain
//! logic (coursers-core) and the concrete hook implementations (hc-b) that
//! will be wired together in the binary (hc-c/hc-d).
//!
//! # Hook lifecycle
//!
//! ```text
//! PreToolUse  ─►  [PreHook₁, PreHook₂, …]  ─►  outcome (Allow | Deny | Rewrite)
//! PostToolUse ─►  [PostHook₁, PostHook₂, …] ─►  outcome (Allow | Filter)
//!              ─►  [Observer₁, Observer₂, …] ─►  side-effects only (no blocking)
//! ```
//!
//! A [`HookChain`] owns zero or more hooks of each type and drives the
//! short-circuit evaluation rules:
//! - `PreHook` chain: first `Deny` or `Rewrite` wins; subsequent hooks are
//!   skipped once the chain is resolved.
//! - `PostHook` chain: all hooks run; last non-`Allow` outcome wins.
//! - `Observer` chain: all observers always run, in order.

use serde_json::Value;

use crate::error::CourserError;

// ---------------------------------------------------------------------------
// Shared context
// ---------------------------------------------------------------------------

/// Metadata shared across all hook invocations.
///
/// Carries the raw tool-use payload alongside decoded convenience fields.
/// Concrete adapters (hc-b) are free to ignore fields they don't need.
#[derive(Debug, Clone)]
pub struct HookContext {
    /// The Claude/Codex tool name, e.g. `"Bash"`, `"Read"`, `"Edit"`.
    pub tool_name: String,
    /// The full, unmodified input payload as received from the harness.
    pub raw_input: Value,
}

impl HookContext {
    /// Convenience constructor for tests.
    pub fn new(tool_name: impl Into<String>, raw_input: Value) -> Self {
        Self {
            tool_name: tool_name.into(),
            raw_input,
        }
    }
}

// ---------------------------------------------------------------------------
// PreHook — PreToolUse port
// ---------------------------------------------------------------------------

/// Outcome returned by a [`PreHook`] implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreHookOutcome {
    /// Let the tool call proceed unchanged.
    Allow,
    /// Block the tool call with a human-readable reason.
    Deny(String),
    /// Allow the tool call but rewrite its command string.
    Rewrite {
        /// The rewritten command.
        command: String,
        /// Optional human-readable explanation emitted as a system message.
        reason: String,
    },
}

/// Port for hooks that intercept tool calls *before* they execute.
///
/// Implementors can inspect the [`HookContext`] and return one of:
/// - [`PreHookOutcome::Allow`] — proceed unchanged
/// - [`PreHookOutcome::Deny`] — block and surface `reason` to the model
/// - [`PreHookOutcome::Rewrite`] — pass a different command to the tool
///
/// # Errors
///
/// Return `Err` only for unexpected I/O or internal failures; business-logic
/// decisions (block, allow) belong in the `Ok(PreHookOutcome)` variants.
pub trait PreHook {
    fn run(&self, ctx: &HookContext) -> Result<PreHookOutcome, CourserError>;
}

// ---------------------------------------------------------------------------
// PostHook — PostToolUse port
// ---------------------------------------------------------------------------

/// The output produced by a tool call, as received from the harness.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    /// Combined stdout / output text (may be empty).
    pub text: String,
    /// Exit code of the underlying process (0 = success).
    pub exit_code: i64,
}

/// Outcome returned by a [`PostHook`] implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostHookOutcome {
    /// Pass the output through unchanged.
    Allow,
    /// Replace the output with a filtered/compressed version.
    Filter(String),
}

/// Port for hooks that observe tool output *after* execution.
///
/// Implementors can suppress, compress, or annotate tool output.
///
/// # Errors
///
/// Same convention as [`PreHook::run`].
pub trait PostHook {
    fn run(&self, ctx: &HookContext, output: &ToolOutput) -> Result<PostHookOutcome, CourserError>;
}

// ---------------------------------------------------------------------------
// Observer — side-effects only
// ---------------------------------------------------------------------------

/// Port for hooks that observe events without influencing the outcome.
///
/// Useful for logging, telemetry, failure-learning state updates, and
/// metrics that should never block a tool call. Errors are non-fatal by
/// convention — the [`HookChain`] logs them but continues.
pub trait Observer {
    fn on_pre(&self, ctx: &HookContext, outcome: &PreHookOutcome) -> Result<(), CourserError>;

    fn on_post(
        &self,
        ctx: &HookContext,
        output: &ToolOutput,
        outcome: &PostHookOutcome,
    ) -> Result<(), CourserError>;
}

// ---------------------------------------------------------------------------
// HookChain
// ---------------------------------------------------------------------------

/// Composable chain of [`PreHook`]s, [`PostHook`]s, and [`Observer`]s.
///
/// # Pre-chain evaluation
///
/// Hooks run in insertion order.  The chain stops at the first `Deny` or
/// `Rewrite` result; remaining hooks are skipped.  All observers receive
/// the resolved outcome.
///
/// # Post-chain evaluation
///
/// All post-hooks run in order; later hooks may override an earlier
/// `Filter` outcome.  Observers always see the final outcome.
pub struct HookChain {
    pre: Vec<Box<dyn PreHook>>,
    post: Vec<Box<dyn PostHook>>,
    observers: Vec<Box<dyn Observer>>,
}

impl Default for HookChain {
    fn default() -> Self {
        Self::new()
    }
}

impl HookChain {
    /// Create an empty chain.
    pub fn new() -> Self {
        Self {
            pre: Vec::new(),
            post: Vec::new(),
            observers: Vec::new(),
        }
    }

    /// Append a pre-hook.
    pub fn with_pre(mut self, hook: impl PreHook + 'static) -> Self {
        self.pre.push(Box::new(hook));
        self
    }

    /// Append a post-hook.
    pub fn with_post(mut self, hook: impl PostHook + 'static) -> Self {
        self.post.push(Box::new(hook));
        self
    }

    /// Append an observer.
    pub fn with_observer(mut self, obs: impl Observer + 'static) -> Self {
        self.observers.push(Box::new(obs));
        self
    }

    /// Run all pre-hooks for a tool-use event.
    ///
    /// Returns the resolved [`PreHookOutcome`].  Observers are notified after
    /// the outcome is determined.
    pub fn run_pre(&self, ctx: &HookContext) -> Result<PreHookOutcome, CourserError> {
        let mut outcome = PreHookOutcome::Allow;

        for hook in &self.pre {
            let result = hook.run(ctx)?;
            match &result {
                PreHookOutcome::Allow => {}
                PreHookOutcome::Deny(_) | PreHookOutcome::Rewrite { .. } => {
                    outcome = result;
                    break;
                }
            }
        }

        for obs in &self.observers {
            // Observer failures are non-fatal; log and continue.
            if let Err(e) = obs.on_pre(ctx, &outcome) {
                eprintln!("[crs observer] pre error: {e}");
            }
        }

        Ok(outcome)
    }

    /// Run all post-hooks for a tool-use event.
    ///
    /// Returns the resolved [`PostHookOutcome`].  Observers are notified after
    /// all post-hooks have run.
    pub fn run_post(
        &self,
        ctx: &HookContext,
        output: &ToolOutput,
    ) -> Result<PostHookOutcome, CourserError> {
        let mut outcome = PostHookOutcome::Allow;

        for hook in &self.post {
            let result = hook.run(ctx, output)?;
            if result != PostHookOutcome::Allow {
                outcome = result;
            }
        }

        for obs in &self.observers {
            if let Err(e) = obs.on_post(ctx, output, &outcome) {
                eprintln!("[crs observer] post error: {e}");
            }
        }

        Ok(outcome)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bash_ctx(command: &str) -> HookContext {
        HookContext::new("Bash", json!({ "command": command }))
    }

    fn success_output() -> ToolOutput {
        ToolOutput {
            text: "ok".into(),
            exit_code: 0,
        }
    }

    // --- Fake hooks --------------------------------------------------------

    struct AlwaysAllow;
    impl PreHook for AlwaysAllow {
        fn run(&self, _ctx: &HookContext) -> Result<PreHookOutcome, CourserError> {
            Ok(PreHookOutcome::Allow)
        }
    }

    struct AlwaysDeny(&'static str);
    impl PreHook for AlwaysDeny {
        fn run(&self, _ctx: &HookContext) -> Result<PreHookOutcome, CourserError> {
            Ok(PreHookOutcome::Deny(self.0.to_string()))
        }
    }

    struct AlwaysRewrite(&'static str);
    impl PreHook for AlwaysRewrite {
        fn run(&self, _ctx: &HookContext) -> Result<PreHookOutcome, CourserError> {
            Ok(PreHookOutcome::Rewrite {
                command: self.0.to_string(),
                reason: "test rewrite".into(),
            })
        }
    }

    struct PassthroughPost;
    impl PostHook for PassthroughPost {
        fn run(
            &self,
            _ctx: &HookContext,
            _output: &ToolOutput,
        ) -> Result<PostHookOutcome, CourserError> {
            Ok(PostHookOutcome::Allow)
        }
    }

    struct FilterPost(&'static str);
    impl PostHook for FilterPost {
        fn run(
            &self,
            _ctx: &HookContext,
            _output: &ToolOutput,
        ) -> Result<PostHookOutcome, CourserError> {
            Ok(PostHookOutcome::Filter(self.0.to_string()))
        }
    }

    /// Counts how many times on_pre / on_post were called.
    struct CountingObserver {
        pre_calls: std::cell::Cell<usize>,
        post_calls: std::cell::Cell<usize>,
    }

    impl CountingObserver {
        fn new() -> Self {
            Self {
                pre_calls: std::cell::Cell::new(0),
                post_calls: std::cell::Cell::new(0),
            }
        }
    }

    impl Observer for CountingObserver {
        fn on_pre(
            &self,
            _ctx: &HookContext,
            _outcome: &PreHookOutcome,
        ) -> Result<(), CourserError> {
            self.pre_calls.set(self.pre_calls.get() + 1);
            Ok(())
        }

        fn on_post(
            &self,
            _ctx: &HookContext,
            _output: &ToolOutput,
            _outcome: &PostHookOutcome,
        ) -> Result<(), CourserError> {
            self.post_calls.set(self.post_calls.get() + 1);
            Ok(())
        }
    }

    // --- Pre-chain tests ---------------------------------------------------

    #[test]
    fn empty_pre_chain_allows() {
        let chain = HookChain::new();
        let outcome = chain.run_pre(&bash_ctx("ls")).unwrap();
        assert_eq!(outcome, PreHookOutcome::Allow);
    }

    #[test]
    fn single_allow_pre_hook() {
        let chain = HookChain::new().with_pre(AlwaysAllow);
        let outcome = chain.run_pre(&bash_ctx("ls")).unwrap();
        assert_eq!(outcome, PreHookOutcome::Allow);
    }

    #[test]
    fn deny_short_circuits_remaining_hooks() {
        // Deny first, then Rewrite — Rewrite should never run.
        let chain = HookChain::new()
            .with_pre(AlwaysDeny("blocked"))
            .with_pre(AlwaysRewrite("nu -c ls"));
        let outcome = chain.run_pre(&bash_ctx("ls")).unwrap();
        assert_eq!(outcome, PreHookOutcome::Deny("blocked".into()));
    }

    #[test]
    fn rewrite_short_circuits_remaining_hooks() {
        let chain = HookChain::new()
            .with_pre(AlwaysRewrite("nu -c ls"))
            .with_pre(AlwaysDeny("should not run"));
        let outcome = chain.run_pre(&bash_ctx("ls")).unwrap();
        assert!(matches!(outcome, PreHookOutcome::Rewrite { .. }));
    }

    #[test]
    fn allow_before_deny_still_denies() {
        let chain = HookChain::new()
            .with_pre(AlwaysAllow)
            .with_pre(AlwaysDeny("second hook denies"));
        let outcome = chain.run_pre(&bash_ctx("ls")).unwrap();
        assert_eq!(outcome, PreHookOutcome::Deny("second hook denies".into()));
    }

    // --- Post-chain tests --------------------------------------------------

    #[test]
    fn empty_post_chain_allows() {
        let chain = HookChain::new();
        let outcome = chain.run_post(&bash_ctx("ls"), &success_output()).unwrap();
        assert_eq!(outcome, PostHookOutcome::Allow);
    }

    #[test]
    fn filter_post_hook_wins_over_allow() {
        let chain = HookChain::new()
            .with_post(PassthroughPost)
            .with_post(FilterPost("filtered"));
        let outcome = chain.run_post(&bash_ctx("ls"), &success_output()).unwrap();
        assert_eq!(outcome, PostHookOutcome::Filter("filtered".into()));
    }

    #[test]
    fn last_filter_post_hook_wins() {
        let chain = HookChain::new()
            .with_post(FilterPost("first"))
            .with_post(FilterPost("last"));
        let outcome = chain.run_post(&bash_ctx("ls"), &success_output()).unwrap();
        assert_eq!(outcome, PostHookOutcome::Filter("last".into()));
    }

    // --- Observer tests ----------------------------------------------------

    #[test]
    fn observer_called_on_pre() {
        use std::sync::Arc;

        let obs = Arc::new(CountingObserver::new());

        // We can't use Arc<T> directly with with_observer (needs 'static + sized),
        // so test via a wrapping approach using a raw pointer-based fake.
        // Instead, build a chain with a simple observer:
        struct TrackingObs(std::cell::RefCell<usize>);
        impl Observer for TrackingObs {
            fn on_pre(&self, _: &HookContext, _: &PreHookOutcome) -> Result<(), CourserError> {
                *self.0.borrow_mut() += 1;
                Ok(())
            }
            fn on_post(
                &self,
                _: &HookContext,
                _: &ToolOutput,
                _: &PostHookOutcome,
            ) -> Result<(), CourserError> {
                Ok(())
            }
        }

        // We need to verify after the chain is consumed, so use a global counter.
        use std::sync::atomic::{AtomicUsize, Ordering};
        static PRE_COUNT: AtomicUsize = AtomicUsize::new(0);

        struct AtomicObs;
        impl Observer for AtomicObs {
            fn on_pre(&self, _: &HookContext, _: &PreHookOutcome) -> Result<(), CourserError> {
                PRE_COUNT.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            fn on_post(
                &self,
                _: &HookContext,
                _: &ToolOutput,
                _: &PostHookOutcome,
            ) -> Result<(), CourserError> {
                Ok(())
            }
        }

        let chain = HookChain::new().with_observer(AtomicObs);
        chain.run_pre(&bash_ctx("ls")).unwrap();
        assert_eq!(PRE_COUNT.load(Ordering::SeqCst), 1);
        chain.run_pre(&bash_ctx("ls")).unwrap();
        assert_eq!(PRE_COUNT.load(Ordering::SeqCst), 2);

        // suppress unused warning
        drop(obs);
    }

    #[test]
    fn observer_called_on_post() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static POST_COUNT: AtomicUsize = AtomicUsize::new(0);

        struct AtomicObs;
        impl Observer for AtomicObs {
            fn on_pre(&self, _: &HookContext, _: &PreHookOutcome) -> Result<(), CourserError> {
                Ok(())
            }
            fn on_post(
                &self,
                _: &HookContext,
                _: &ToolOutput,
                _: &PostHookOutcome,
            ) -> Result<(), CourserError> {
                POST_COUNT.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }

        let chain = HookChain::new().with_observer(AtomicObs);
        chain.run_post(&bash_ctx("ls"), &success_output()).unwrap();
        assert_eq!(POST_COUNT.load(Ordering::SeqCst), 1);
    }

    // --- Composition test --------------------------------------------------

    #[test]
    fn full_chain_composition() {
        // Allow -> Allow -> Deny: should resolve to Deny.
        // Post: Allow -> Filter: should resolve to Filter.
        // Observer: should be called once for each.
        use std::sync::atomic::{AtomicUsize, Ordering};
        static OBS_PRE: AtomicUsize = AtomicUsize::new(0);
        static OBS_POST: AtomicUsize = AtomicUsize::new(0);

        struct CompositionObs;
        impl Observer for CompositionObs {
            fn on_pre(&self, _: &HookContext, _: &PreHookOutcome) -> Result<(), CourserError> {
                OBS_PRE.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            fn on_post(
                &self,
                _: &HookContext,
                _: &ToolOutput,
                _: &PostHookOutcome,
            ) -> Result<(), CourserError> {
                OBS_POST.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }

        let chain = HookChain::new()
            .with_pre(AlwaysAllow)
            .with_pre(AlwaysAllow)
            .with_pre(AlwaysDeny("composition test deny"))
            .with_post(PassthroughPost)
            .with_post(FilterPost("composition filter"))
            .with_observer(CompositionObs);

        let pre_outcome = chain.run_pre(&bash_ctx("echo hi")).unwrap();
        assert_eq!(
            pre_outcome,
            PreHookOutcome::Deny("composition test deny".into())
        );
        assert_eq!(OBS_PRE.load(Ordering::SeqCst), 1);

        let post_outcome = chain
            .run_post(&bash_ctx("echo hi"), &success_output())
            .unwrap();
        assert_eq!(
            post_outcome,
            PostHookOutcome::Filter("composition filter".into())
        );
        assert_eq!(OBS_POST.load(Ordering::SeqCst), 1);
    }
}
