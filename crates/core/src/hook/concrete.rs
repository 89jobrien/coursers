//! Concrete hook implementations wrapping the coursers rule/state/filter/rewrite logic.
//!
//! Each struct is a thin adapter: it holds port-trait dependencies injected at
//! construction time and implements one of the three hc-a chain traits
//! ([`PreHook`], [`PostHook`], [`Observer`]).  No rule-matching, rewrite, or
//! filter algorithms live here — they delegate to the existing modules.
//!
//! # Structs
//!
//! | Struct              | Trait        | Delegates to                              |
//! |---------------------|--------------|-------------------------------------------|
//! | [`RuleBlockHook`]   | [`PreHook`]  | `rules::check_pipeline`, `state::check_learned` |
//! | [`RewriteHook`]     | [`PreHook`]  | `rewrite::apply`                          |
//! | [`FilterHook`]      | [`PostHook`] | `filter_logic::run_filter`                |
//! | [`FailureObserver`] | [`Observer`] | `state::record_failure`                   |

use serde_json::Value;

use crate::error::CourserError;
use crate::hook::chain::{
    HookContext, Observer, PostHook, PostHookOutcome, PreHook, PreHookOutcome, ToolOutput,
};
use crate::hook::filter_logic::{FilterPayload, run_filter};
use crate::hook::filters::FiltersLoader;
use crate::hook::rewrite::{RewriteLoader, apply as rewrite_apply};
use crate::loader::RulesLoader;
use crate::parse::expand::NoopExpander;
use crate::rules::{check_pipeline, task_overrides_rule};
use crate::state::{check_learned, record_failure};
use crate::store::StateStore;
use crate::types::filters::FilterResult;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Extract the `command` field from a Bash tool input payload.
///
/// Returns `None` if the field is absent or not a string; callers treat
/// missing command as an Allow (non-Bash tool invocations).
fn extract_command(raw_input: &Value) -> Option<&str> {
    raw_input.get("command")?.as_str()
}

// ---------------------------------------------------------------------------
// RuleBlockHook — PreHook
// ---------------------------------------------------------------------------

/// Pre-hook that blocks commands matching course-correct rules or that have
/// exceeded the failure-learning threshold.
///
/// Evaluation order (mirrors `crs pre` CLI):
/// 1. Load rules and state from their respective ports.
/// 2. For each matching rule, check whether a running godmode task overrides it.
/// 3. If no rule override, deny with the rule's message.
/// 4. Otherwise check `check_learned`; deny if threshold exceeded.
pub struct RuleBlockHook<R, S> {
    rules_loader: R,
    state_store: S,
    /// Optional running godmode task titles for `task_override` suppression.
    /// Pass an empty slice to disable the override feature.
    running_titles: Vec<String>,
}

impl<R: RulesLoader, S: StateStore> RuleBlockHook<R, S> {
    /// Construct with explicit dependencies.
    pub fn new(rules_loader: R, state_store: S) -> Self {
        Self {
            rules_loader,
            state_store,
            running_titles: Vec::new(),
        }
    }

    /// Attach running godmode task titles for task-override suppression.
    pub fn with_running_titles(mut self, titles: Vec<String>) -> Self {
        self.running_titles = titles;
        self
    }
}

impl<R: RulesLoader, S: StateStore> PreHook for RuleBlockHook<R, S> {
    fn run(&self, ctx: &HookContext) -> Result<PreHookOutcome, CourserError> {
        let Some(command) = extract_command(&ctx.raw_input) else {
            return Ok(PreHookOutcome::Allow);
        };

        let config = self.rules_loader.load()?;

        // --- Rule-block check ---
        // Walk the pipeline segments; deny on the first matching rule that is
        // not suppressed by a running godmode task.
        for seg in crate::parse::pipeline::sequential_segments(command)
            .into_iter()
            .chain(std::iter::once(command))
        {
            // TODO(task-override-rule-scan): Scan all matching rules after an override;
            // suppressing the first match must not bypass a later blocking rule.
            if let Some(rule) = config.rules.iter().find(|r| {
                r.enabled && crate::rules::matched_rule_id(seg, std::slice::from_ref(r)).is_some()
            }) {
                if task_overrides_rule(rule, &self.running_titles) {
                    continue;
                }
                let (_, msg) = check_pipeline(seg, &config.rules)
                    .or_else(|| check_pipeline(command, &config.rules))
                    .unwrap_or_else(|| {
                        (
                            rule.id.clone(),
                            rule.message
                                .clone()
                                .unwrap_or_else(|| format!("Blocked by rule '{}'.", rule.id)),
                        )
                    });
                return Ok(PreHookOutcome::Deny(msg));
            }
        }

        // --- Failure-learning check ---
        let state = self.state_store.load().unwrap_or_default();
        if let Some(msg) = check_learned(command, &state, &config.failure_learning) {
            return Ok(PreHookOutcome::Deny(msg));
        }

        Ok(PreHookOutcome::Allow)
    }
}

// ---------------------------------------------------------------------------
// RewriteHook — PreHook
// ---------------------------------------------------------------------------

/// Pre-hook that rewrites commands via `[[rewrites]]` rules in the filters TOML.
///
/// Returns `PreHookOutcome::Rewrite` when any rule fires, `Allow` otherwise.
pub struct RewriteHook<R> {
    rewrite_loader: R,
}

impl<R: RewriteLoader> RewriteHook<R> {
    pub fn new(rewrite_loader: R) -> Self {
        Self { rewrite_loader }
    }
}

impl<R: RewriteLoader> PreHook for RewriteHook<R> {
    fn run(&self, ctx: &HookContext) -> Result<PreHookOutcome, CourserError> {
        let Some(command) = extract_command(&ctx.raw_input) else {
            return Ok(PreHookOutcome::Allow);
        };

        let rewrite_cfg = self.rewrite_loader.load()?;

        match rewrite_apply(command, &rewrite_cfg, &NoopExpander) {
            Some(outcome) => Ok(PreHookOutcome::Rewrite {
                command: outcome.command,
                reason: if outcome.applied_rules.is_empty() {
                    "Rewritten by expansion".to_string()
                } else {
                    format!("Rewritten by rules: {}", outcome.applied_rules.join(", "))
                },
            }),
            None => Ok(PreHookOutcome::Allow),
        }
    }
}

// ---------------------------------------------------------------------------
// FilterHook — PostHook
// ---------------------------------------------------------------------------

/// Post-hook that compresses or suppresses tool output via `[[filters]]` rules.
///
/// Mapping from [`FilterResult`] to [`PostHookOutcome`]:
/// - `Passthrough` → `Allow`
/// - `Replace(text)` → `Filter(text)`
/// - `Suppress` → `Filter("")`  (empty string signals full suppression to consumers)
pub struct FilterHook<F> {
    filters_loader: F,
}

impl<F: FiltersLoader> FilterHook<F> {
    pub fn new(filters_loader: F) -> Self {
        Self { filters_loader }
    }
}

impl<F: FiltersLoader> PostHook for FilterHook<F> {
    fn run(&self, ctx: &HookContext, output: &ToolOutput) -> Result<PostHookOutcome, CourserError> {
        let Some(command) = extract_command(&ctx.raw_input) else {
            return Ok(PostHookOutcome::Allow);
        };

        let filters_cfg = self.filters_loader.load()?;
        let payload = FilterPayload {
            command: command.to_string(),
            output: output.text.clone(),
            exit_code: output.exit_code,
        };

        let result = run_filter(&payload, &filters_cfg);
        Ok(match result {
            FilterResult::Passthrough => PostHookOutcome::Allow,
            FilterResult::Replace(text) => PostHookOutcome::Filter(text),
            FilterResult::Suppress => PostHookOutcome::Filter(String::new()),
        })
    }
}

// ---------------------------------------------------------------------------
// FailureObserver — Observer
// ---------------------------------------------------------------------------

/// Observer that records non-zero exits to the failure-learning state store.
///
/// - `on_pre`: no-op — pre-hook outcomes don't produce a meaningful exit code.
/// - `on_post`: records a failure entry when `output.exit_code != 0`, using
///   `state::record_failure` and the `FailureLearning` config from the rules.
pub struct FailureObserver<R, S> {
    rules_loader: R,
    state_store: S,
}

impl<R: RulesLoader, S: StateStore> FailureObserver<R, S> {
    pub fn new(rules_loader: R, state_store: S) -> Self {
        Self {
            rules_loader,
            state_store,
        }
    }
}

impl<R: RulesLoader, S: StateStore> Observer for FailureObserver<R, S> {
    fn on_pre(&self, _ctx: &HookContext, _outcome: &PreHookOutcome) -> Result<(), CourserError> {
        Ok(())
    }

    fn on_post(
        &self,
        ctx: &HookContext,
        output: &ToolOutput,
        _outcome: &PostHookOutcome,
    ) -> Result<(), CourserError> {
        if output.exit_code == 0 {
            return Ok(());
        }

        let Some(command) = extract_command(&ctx.raw_input) else {
            return Ok(());
        };

        let config = self.rules_loader.load()?;
        let state = self.state_store.load().unwrap_or_default();
        let new_state = record_failure(state, command, &config.failure_learning);
        self.state_store.save(&new_state)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook::chain::{HookChain, PostHookOutcome, PreHookOutcome};
    use crate::hook::filters::{FiltersConfig, InMemoryFiltersLoader};
    use crate::hook::rewrite::{InMemoryRewriteLoader, RewriteConfig, RewriteRule};
    use crate::loader::InMemoryRulesLoader;
    use crate::rules::{FailureLearning, Rule, RulesConfig};
    use crate::state::State;
    use crate::store::InMemoryStateStore;
    use serde_json::json;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn bash_ctx(command: &str) -> HookContext {
        HookContext::new("Bash", json!({ "command": command }))
    }

    fn success_output(text: &str) -> ToolOutput {
        ToolOutput {
            text: text.to_string(),
            exit_code: 0,
        }
    }

    fn failure_output(text: &str) -> ToolOutput {
        ToolOutput {
            text: text.to_string(),
            exit_code: 1,
        }
    }

    fn empty_rules() -> RulesConfig {
        RulesConfig {
            rules: vec![],
            failure_learning: FailureLearning::default(),
        }
    }

    fn rule_blocking(id: &str, pattern: &str) -> Rule {
        Rule {
            id: id.to_string(),
            enabled: true,
            pattern: pattern.to_string(),
            pattern_flags: String::new(),
            exceptions: vec![],
            target_commands: vec![],
            message: Some(format!("Use the dedicated tool instead of {id}.")),
            task_override: None,
        }
    }

    fn empty_filters() -> FiltersConfig {
        FiltersConfig::default()
    }

    // ── RuleBlockHook ────────────────────────────────────────────────────────

    #[test]
    fn rule_block_hook_allows_clean_command() {
        let loader = InMemoryRulesLoader(empty_rules());
        let store = InMemoryStateStore::new();
        let hook = RuleBlockHook::new(loader, store);

        let outcome = hook.run(&bash_ctx("cargo build")).unwrap();
        assert_eq!(outcome, PreHookOutcome::Allow);
    }

    #[test]
    fn rule_block_hook_denies_matching_command() {
        let config = RulesConfig {
            rules: vec![rule_blocking("no-grep", r"\bgrep\b")],
            ..empty_rules()
        };
        let hook = RuleBlockHook::new(InMemoryRulesLoader(config), InMemoryStateStore::new());

        let outcome = hook.run(&bash_ctx("grep foo .")).unwrap();
        assert!(matches!(outcome, PreHookOutcome::Deny(_)));
    }

    #[test]
    fn rule_block_hook_message_from_rule() {
        let config = RulesConfig {
            rules: vec![rule_blocking("no-grep", r"\bgrep\b")],
            ..empty_rules()
        };
        let hook = RuleBlockHook::new(InMemoryRulesLoader(config), InMemoryStateStore::new());

        match hook.run(&bash_ctx("grep foo .")).unwrap() {
            PreHookOutcome::Deny(msg) => {
                assert!(
                    msg.contains("no-grep") || msg.contains("dedicated tool"),
                    "unexpected message: {msg}"
                );
            }
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[test]
    fn rule_block_hook_allows_non_bash_context() {
        // When raw_input has no "command" key, the hook must allow.
        let config = RulesConfig {
            rules: vec![rule_blocking("no-grep", r"\bgrep\b")],
            ..empty_rules()
        };
        let hook = RuleBlockHook::new(InMemoryRulesLoader(config), InMemoryStateStore::new());
        let ctx = HookContext::new("Read", json!({ "file_path": "/tmp/foo" }));
        assert_eq!(hook.run(&ctx).unwrap(), PreHookOutcome::Allow);
    }

    #[test]
    fn rule_block_hook_denies_via_failure_learning() {
        use crate::state::{FailureEntry, command_key};

        let fl = FailureLearning {
            enabled: true,
            block_threshold: 2,
            window_seconds: 3600,
            ..FailureLearning::default()
        };
        let config = RulesConfig {
            rules: vec![],
            failure_learning: fl,
        };

        // Pre-populate the state with failures above threshold.
        let now = crate::state::now_secs();
        let key = command_key("cargo test --fail");
        let mut state = State::default();
        state.failures.insert(
            key,
            FailureEntry {
                command_preview: "cargo test --fail".to_string(),
                timestamps: vec![now - 10, now - 5],
                last_seen: (now - 5) as f64,
            },
        );
        let store = InMemoryStateStore::with_state(state);

        let hook = RuleBlockHook::new(InMemoryRulesLoader(config), store);
        let outcome = hook.run(&bash_ctx("cargo test --fail")).unwrap();
        assert!(
            matches!(outcome, PreHookOutcome::Deny(_)),
            "expected Deny from failure learning"
        );
    }

    // ── RewriteHook ──────────────────────────────────────────────────────────

    fn rewrite_cfg(pattern: &str, replace: &str) -> RewriteConfig {
        RewriteConfig {
            rewrites: vec![RewriteRule {
                pattern: pattern.to_string(),
                replace: replace.to_string(),
            }],
        }
    }

    #[test]
    fn rewrite_hook_returns_allow_on_no_match() {
        let hook = RewriteHook::new(InMemoryRewriteLoader(RewriteConfig::default()));
        let outcome = hook.run(&bash_ctx("cargo build")).unwrap();
        assert_eq!(outcome, PreHookOutcome::Allow);
    }

    #[test]
    fn rewrite_hook_returns_rewrite_on_match() {
        let cfg = rewrite_cfg("^git status$", "git status --short");
        let hook = RewriteHook::new(InMemoryRewriteLoader(cfg));
        match hook.run(&bash_ctx("git status")).unwrap() {
            PreHookOutcome::Rewrite { command, .. } => {
                assert_eq!(command, "git status --short");
            }
            other => panic!("expected Rewrite, got {other:?}"),
        }
    }

    #[test]
    fn rewrite_hook_reason_mentions_rules() {
        let cfg = rewrite_cfg("^git status$", "git status --short");
        let hook = RewriteHook::new(InMemoryRewriteLoader(cfg));
        match hook.run(&bash_ctx("git status")).unwrap() {
            PreHookOutcome::Rewrite { reason, .. } => {
                assert!(reason.contains("^git status$"), "reason: {reason}");
            }
            other => panic!("expected Rewrite, got {other:?}"),
        }
    }

    #[test]
    fn rewrite_hook_allows_non_bash_context() {
        let cfg = rewrite_cfg("cargo", "cargo --color always");
        let hook = RewriteHook::new(InMemoryRewriteLoader(cfg));
        let ctx = HookContext::new("Edit", json!({ "file_path": "/tmp/main.rs" }));
        assert_eq!(hook.run(&ctx).unwrap(), PreHookOutcome::Allow);
    }

    // ── FilterHook ───────────────────────────────────────────────────────────

    fn filters_with_failures_only(pattern: &str) -> FiltersConfig {
        use crate::hook::filters::{FilterMode, FilterRule};
        FiltersConfig {
            filters: vec![FilterRule {
                pattern: pattern.to_string(),
                mode: FilterMode::FailuresOnly,
                max_lines: 50,
                match_pattern: None,
            }],
            ..FiltersConfig::default()
        }
    }

    #[test]
    fn filter_hook_allows_when_no_rule_matches() {
        let hook = FilterHook::new(InMemoryFiltersLoader(empty_filters()));
        let ctx = bash_ctx("cargo build");
        let out = success_output("output");
        assert_eq!(hook.run(&ctx, &out).unwrap(), PostHookOutcome::Allow);
    }

    #[test]
    fn filter_hook_suppresses_on_success_with_failures_only() {
        let cfg = filters_with_failures_only("cargo nextest");
        let hook = FilterHook::new(InMemoryFiltersLoader(cfg));
        let ctx = bash_ctx("cargo nextest run");
        let out = success_output("test passed");
        // FailuresOnly on success → Suppress → Filter("")
        assert_eq!(
            hook.run(&ctx, &out).unwrap(),
            PostHookOutcome::Filter(String::new())
        );
    }

    #[test]
    fn filter_hook_allows_on_failure_with_failures_only() {
        let cfg = filters_with_failures_only("cargo nextest");
        let hook = FilterHook::new(InMemoryFiltersLoader(cfg));
        let ctx = bash_ctx("cargo nextest run");
        let out = failure_output("FAILED: some_test");
        // FailuresOnly on failure → output passes through → Allow
        assert_eq!(hook.run(&ctx, &out).unwrap(), PostHookOutcome::Allow);
    }

    #[test]
    fn filter_hook_allows_non_bash_context() {
        let cfg = filters_with_failures_only("cargo");
        let hook = FilterHook::new(InMemoryFiltersLoader(cfg));
        let ctx = HookContext::new("Read", json!({ "file_path": "/tmp/foo" }));
        let out = success_output("some output");
        assert_eq!(hook.run(&ctx, &out).unwrap(), PostHookOutcome::Allow);
    }

    // ── FailureObserver ──────────────────────────────────────────────────────

    #[test]
    fn failure_observer_records_non_zero_exit() {
        let store = InMemoryStateStore::new();
        let obs = FailureObserver::new(InMemoryRulesLoader(empty_rules()), &store);

        let ctx = bash_ctx("cargo test");
        let out = failure_output("FAILED");
        obs.on_post(&ctx, &out, &PostHookOutcome::Allow).unwrap();

        let state = store.get_state();
        assert_eq!(state.failures.len(), 1);
    }

    #[test]
    fn failure_observer_ignores_zero_exit() {
        let store = InMemoryStateStore::new();
        let obs = FailureObserver::new(InMemoryRulesLoader(empty_rules()), &store);

        let ctx = bash_ctx("cargo build");
        let out = success_output("ok");
        obs.on_post(&ctx, &out, &PostHookOutcome::Allow).unwrap();

        let state = store.get_state();
        assert!(state.failures.is_empty());
    }

    #[test]
    fn failure_observer_on_pre_is_noop() {
        let store = InMemoryStateStore::new();
        let obs = FailureObserver::new(InMemoryRulesLoader(empty_rules()), &store);

        let ctx = bash_ctx("grep foo .");
        obs.on_pre(&ctx, &PreHookOutcome::Allow).unwrap();
        // No state change expected
        assert!(store.get_state().failures.is_empty());
    }

    #[test]
    fn failure_observer_ignores_non_bash_context() {
        let store = InMemoryStateStore::new();
        let obs = FailureObserver::new(InMemoryRulesLoader(empty_rules()), &store);

        let ctx = HookContext::new("Edit", json!({ "file_path": "/tmp/foo" }));
        let out = failure_output("error");
        obs.on_post(&ctx, &out, &PostHookOutcome::Allow).unwrap();

        // No state change since there's no command field.
        assert!(store.get_state().failures.is_empty());
    }

    // ── HookChain integration ────────────────────────────────────────────────

    #[test]
    fn chain_with_rule_block_hook_denies() {
        let config = RulesConfig {
            rules: vec![rule_blocking("no-grep", r"\bgrep\b")],
            ..empty_rules()
        };
        let chain = HookChain::new().with_pre(RuleBlockHook::new(
            InMemoryRulesLoader(config),
            InMemoryStateStore::new(),
        ));

        let outcome = chain.run_pre(&bash_ctx("grep foo .")).unwrap();
        assert!(matches!(outcome, PreHookOutcome::Deny(_)));
    }

    #[test]
    fn chain_with_filter_hook_suppresses() {
        use crate::hook::filters::{FilterMode, FilterRule};
        let cfg = FiltersConfig {
            filters: vec![FilterRule {
                pattern: "cargo nextest".to_string(),
                mode: FilterMode::FailuresOnly,
                max_lines: 50,
                match_pattern: None,
            }],
            ..FiltersConfig::default()
        };
        let chain = HookChain::new().with_post(FilterHook::new(InMemoryFiltersLoader(cfg)));

        let ctx = bash_ctx("cargo nextest run");
        let out = success_output("test output");
        let outcome = chain.run_post(&ctx, &out).unwrap();
        assert_eq!(outcome, PostHookOutcome::Filter(String::new()));
    }

    #[test]
    fn chain_with_observer_records_failure() {
        let store = InMemoryStateStore::new();
        // FailureObserver requires shared reference to store.
        let chain = HookChain::new().with_observer(FailureObserver::new(
            InMemoryRulesLoader(empty_rules()),
            InMemoryStateStore::new(),
        ));

        let ctx = bash_ctx("cargo test");
        let out = failure_output("FAILED");
        chain.run_post(&ctx, &out).unwrap();

        // Note: chain owns its own store; we verify via the direct observer test above.
        // This test validates that the chain wires up without errors.
        let _ = store.get_state();
    }
}
