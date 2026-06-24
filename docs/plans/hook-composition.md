# In-Process Hook Composition

## Summary

Replace the current multi-binary hook architecture (coursers pre, coursers
post, crs rewrite, crs filter as separate processes) with a single in-process
hook chain using trait-object composition. Inspired by seaography's
`LifecycleHooksInterface` pattern.

## Motivation

Today each hook invocation is a cold-start process: parse stdin JSON, load
config from disk, evaluate one concern, write stdout, exit. Problems:

1. **Redundant config loads** — `coursers pre` and `crs rewrite` both load
   rules from the same file in separate processes
2. **Fragile ordering** — hook evaluation order is controlled by array
   position in `settings.json`, not by the hooks themselves
3. **No composition** — hooks can't see each other's decisions (e.g.,
   task-override can't suppress a rule-blocker without being a separate
   process that runs first)
4. **Cold-start cost** — regex compilation on every invocation (~2ms each
   for `coursers pre` + `crs rewrite` = 4ms before the command even runs)
5. **Testability** — integration testing requires subprocess spawning

## Non-goals

- Async hooks. The hot path must stay <5ms synchronous. No futures, no
  tokio, no `Pin<Box<dyn Future>>`. Plain `&self` → decision vtable dispatch.
- Proc macros. Traits are simple enough for native dyn compatibility —
  no `#[async_trait]` needed.
- Dynamic loading (dylib/wasm). Static compilation for now. Plugin
  extensibility is a future concern.
- Replacing Claude Code's hook protocol. The binary still reads stdin JSON
  and writes stdout JSON. The composition is internal.

## Design

### Core types

```rust
// crs-core/src/hook/chain.rs

use serde_json::Value;

/// Context available to all hooks during evaluation.
pub struct HookContext<'a> {
    pub command: &'a str,
    pub tool_name: &'a str,
    pub cwd: &'a str,
    pub session_id: Option<&'a str>,
    /// Only populated for post-hooks.
    pub exit_code: Option<i64>,
    /// Only populated for post-hooks. Owned to allow mutation by chain.
    pub output: Option<String>,
}

/// Structured block reason — keeps domain free of presentation concerns.
/// TODO(joe::JOB-475): error type mapping — binary layer maps to JSON
#[derive(Debug, Clone)]
pub struct BlockReason {
    pub rule_id: String,
    pub message: String,
    pub suggestion: Option<String>,
}

/// Decision from a pre-hook.
#[derive(Debug, Clone)]
pub enum PreDecision {
    /// No opinion — continue to next hook.
    Continue,
    /// Allow unconditionally — skip remaining pre-hooks.
    Allow,
    /// Block with structured reason.
    Block(BlockReason),
    /// Rewrite the command string.
    Rewrite { command: String, description: Option<String> },
    /// Swap to a different Claude Code tool.
    SwapTool { tool_name: String, tool_input: Value },
}

/// Decision from a post-hook.
#[derive(Debug, Clone)]
pub enum PostDecision {
    /// No opinion — pass output unchanged to next hook.
    Continue,
    /// Replace output.
    Transform(String),
    /// Suppress output entirely.
    Suppress,
}
```

### Trait definitions

```rust
/// A pre-tool-use hook. Must be dyn-compatible:
/// - no generics on methods
/// - no async
/// - &self only
pub trait PreHook: Send + Sync {
    fn name(&self) -> &str;
    fn evaluate(&self, ctx: &HookContext) -> PreDecision;
}

/// A post-tool-use hook that transforms output.
/// TODO(joe::JOB-475): ISP — separate from Observer trait
pub trait PostHook: Send + Sync {
    fn name(&self) -> &str;
    fn evaluate(&self, ctx: &HookContext) -> PostDecision;
}

/// A side-effect-only hook. Always runs regardless of prior decisions.
/// Implements ISP: observers don't carry dead evaluate() methods.
/// TODO(joe::JOB-475): ISP — separate from PostHook trait
pub trait Observer: Send + Sync {
    fn name(&self) -> &str;
    fn observe(&self, ctx: &HookContext);
}
```

**ISP rationale**: `FailureRecorder` and `CaptureRecorder` only observe —
they never transform output. `OutputFilter` and `Redactor` only transform —
they have no side-effects. Forcing both into one trait violates Interface
Segregation.

**Why `Send + Sync`**: Even though hooks are synchronous today, requiring
these bounds keeps the door open for a persistent-process model where the
chain is shared across threads (e.g., a hook server handling concurrent
requests).

**Why `Continue` vs `Allow`**: `Continue` means "I have no opinion, ask the
next hook." `Allow` means "I'm explicitly permitting this, skip remaining
hooks." This enables `TaskOverride` to short-circuit `RuleBlocker` without
the override needing to know which rules exist.

### Chain compositor

```rust
/// TODO(joe::JOB-475): composition root for hook system
pub struct HookChain {
    pre: Vec<Box<dyn PreHook>>,
    observers: Vec<Box<dyn Observer>>,
    post: Vec<Box<dyn PostHook>>,
}

impl HookChain {
    pub fn new() -> Self {
        Self { pre: vec![], observers: vec![], post: vec![] }
    }

    pub fn pre(mut self, hook: impl PreHook + 'static) -> Self {
        self.pre.push(Box::new(hook));
        self
    }

    pub fn observer(mut self, hook: impl Observer + 'static) -> Self {
        self.observers.push(Box::new(hook));
        self
    }

    pub fn post(mut self, hook: impl PostHook + 'static) -> Self {
        self.post.push(Box::new(hook));
        self
    }

    /// Evaluate pre-hooks. Short-circuits on Block/Rewrite/SwapTool/Allow.
    pub fn run_pre(&self, ctx: &HookContext) -> PreDecision {
        for hook in &self.pre {
            match hook.evaluate(ctx) {
                PreDecision::Continue => continue,
                decision => return decision,
            }
        }
        PreDecision::Continue // all hooks passed — allow implicitly
    }

    /// Run post-hook pipeline:
    /// 1. All observers fire unconditionally (side-effects).
    /// 2. Post-hooks evaluate in order; transforms compose, suppress
    ///    short-circuits.
    pub fn run_post(&self, ctx: &mut HookContext) -> PostDecision {
        // Observers always run — ISP: separate from transform concern
        for obs in &self.observers {
            obs.observe(ctx);
        }

        let mut final_decision = PostDecision::Continue;

        for hook in &self.post {
            if matches!(final_decision, PostDecision::Suppress) {
                break;
            }

            match hook.evaluate(ctx) {
                PostDecision::Continue => {}
                PostDecision::Suppress => {
                    final_decision = PostDecision::Suppress;
                }
                PostDecision::Transform(new_output) => {
                    ctx.output = Some(new_output.clone());
                    final_decision = PostDecision::Transform(new_output);
                }
            }
        }

        final_decision
    }
}
```

### Concrete hooks (refactored from current code)

| Hook struct       | Trait    | Current source         | Responsibility                             |
| ----------------- | -------- | ---------------------- | ------------------------------------------ |
| `TaskOverride`    | PreHook  | new (Phase 4)          | Suppress rules when godmode task matches   |
| `RuleBlocker`     | PreHook  | `pre.rs` lines 98-123  | Block commands matching predefined rules   |
| `FailureLearner`  | PreHook  | `pre.rs` lines 125-131 | Block commands exceeding failure threshold |
| `ToolSwapper`     | PreHook  | `tool_swap.rs`         | Rewrite cat/head/tail/find to Read/Glob    |
| `CommandRewriter` | PreHook  | `crs rewrite`          | Regex rewrites from filters.toml           |
| `FailureRecorder` | Observer | `post.rs`              | Record non-zero exits to state             |
| `CaptureRecorder` | Observer | `pre.rs` capture logic | Record suggestion pairs for fine-tuning    |
| `OutputFilter`    | PostHook | `crs filter`           | Truncate/suppress/error-filter output      |
| `Redactor`        | PostHook | `filters.rs` redaction | Replace sensitive lines with [REDACTED]    |

### Default chain assembly

```rust
pub fn build_chain(config: &AppConfig) -> HookChain {
    let mut chain = HookChain::new();

    // --- Pre-hooks (order matters) ---

    // 1. Task overrides first — can Allow to skip all blockers
    if config.godmode_enabled {
        chain = chain.pre(TaskOverride::from_cache());
    }

    // 2. Rule blocker
    chain = chain.pre(RuleBlocker::new(&config.rules));

    // 3. Failure learner
    if config.failure_learning.enabled {
        chain = chain.pre(FailureLearner::new(&config.failure_learning));
    }

    // 4. Tool swapper (rewrites cat→Read etc.)
    chain = chain.pre(ToolSwapper::new(&config.tool_swap));

    // 5. Regex rewriter (from filters.toml [[rewrites]])
    chain = chain.pre(CommandRewriter::new(&config.rewrites));

    // --- Observers (side-effects, always run) ---
    // TODO(joe::JOB-475): ISP — observers never transform output

    if config.failure_learning.enabled {
        chain = chain.observer(FailureRecorder::new(&config.failure_learning));
    }
    chain = chain.observer(CaptureRecorder::new());

    // --- Post-hooks (output transformers) ---
    // TODO(joe::JOB-475): ISP — post-hooks never have side-effects

    if !config.obfsck_filters.filters.is_empty() {
        chain = chain.post(Redactor::new(&config.obfsck_filters));
    }
    chain = chain.post(OutputFilter::new(&config.filters));

    chain
}
```

### Binary entry point (unchanged external interface)

```rust
// coursers/src/main.rs

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    let config = AppConfig::load(); // single config load
    let chain = build_chain(&config);

    let payload = read_stdin();
    let mut ctx = HookContext::from_payload(&payload);

    match mode.as_str() {
        "pre" => {
            // TODO(joe::JOB-475): binary layer maps BlockReason → JSON
            match chain.run_pre(&ctx) {
                PreDecision::Continue => {} // exit 0, no output
                PreDecision::Allow => {}
                PreDecision::Block(reason) => deny(&reason.message),
                PreDecision::Rewrite { command, description } => {
                    emit_rewrite(&command, description.as_deref());
                }
                PreDecision::SwapTool { tool_name, tool_input } => {
                    emit_swap(&tool_name, &tool_input);
                }
            }
        }
        "post" => {
            match chain.run_post(&mut ctx) {
                PostDecision::Continue => {} // exit 0, no output
                PostDecision::Suppress => emit_suppress(),
                PostDecision::Transform(output) => emit_transform(&output),
            }
        }
        _ => {}
    }
}
```

## Key design decisions

### Why not async

The `async-trait` crate (dtolnay) exists to make async fns work with
`dyn Trait` by boxing futures. Coursers doesn't need this because:

- Hot path budget is <5ms. Async adds scheduling overhead.
- No I/O in the critical path — config is pre-loaded, state file reads
  are <0.1ms for small JSON.
- `Box<dyn PreHook>` with synchronous `evaluate(&self)` has zero
  allocation per call (vtable dispatch only).
- No Send/Sync bound complexity from async. Traits are natively
  dyn-compatible without proc macros.

If a future hook needs network I/O (e.g., checking a remote policy
server), it should use a timeout-bounded blocking call or be excluded
from the synchronous chain and run as a separate async sidecar.

### Why `observe()` on PostHook

Side-effects (recording failures, capturing suggestion pairs) must run
regardless of whether a prior hook decided to suppress output. Separating
`observe()` from `evaluate()` ensures side-effects are never accidentally
skipped by chain short-circuiting.

### Why `Continue` instead of `Allow` as default

Most hooks have no opinion about most commands. `Continue` means "I didn't
match, ask the next one." This is the seaography pattern — iterate until
someone has an opinion. The chain implicitly allows if no hook objects.

`Allow` is an active decision: "I'm overriding downstream blockers." Only
`TaskOverride` uses this today.

### Regex compilation strategy

Today: regexes compile on every process invocation (~1-2ms for 20 rules).
With in-process composition, compile once at chain construction:

```rust
pub struct RuleBlocker {
    compiled: Vec<(CompiledRule, String)>, // (regex, message)
}

impl RuleBlocker {
    pub fn new(rules: &[Rule]) -> Self {
        let compiled = rules.iter()
            .filter(|r| r.enabled)
            .filter_map(|r| {
                Regex::new(&r.pattern).ok()
                    .map(|re| (CompiledRule { re, exceptions: ... }, r.message.clone()))
            })
            .collect();
        Self { compiled }
    }
}
```

This moves regex compilation from per-invocation to per-config-load.
In a persistent-process model, this happens once at startup.

## Migration plan

### Phase A: Add traits to crs-core (library only)

- Add `crs-core/src/hook/chain.rs` with `PreHook`, `PostHook`, `Observer`, `HookChain`
- Add `crs-core/src/hook/decision.rs` with `PreDecision`, `PostDecision`, `BlockReason`
- No changes to existing code. New module, new tests.
- TODO(joe::JOB-475): start here — traits + chain + unit tests

### Phase B: Implement hook structs

- `RuleBlocker` wrapping existing `rules::check_pipeline` → returns `BlockReason`
- `FailureLearner` wrapping existing `state::check_learned` → returns `BlockReason`
- `ToolSwapper` wrapping existing `tool_swap::apply`
- `OutputFilter` wrapping existing filter logic (PostHook)
- `FailureRecorder` wrapping existing `state::record_failure` (Observer)
- `CaptureRecorder` wrapping capture logic (Observer)
- `Redactor` wrapping existing `apply_redaction` (PostHook)
- Each struct unit-tested independently via the trait interface
- TODO(joe::JOB-475): ensure RuleBlocker emits BlockReason with rule_id + suggestion

### Phase C: Unified config loading

- New `AppConfig` struct that combines `RulesConfig` + `FiltersConfig` +
  `ToolSwapConfig` + `ObfsckFilters`
- Single load at process start
- `build_chain(&AppConfig) -> HookChain` assembles the full chain

### Phase D: Wire into binaries

- `coursers pre` internally calls `chain.run_pre()` instead of direct
  `rules::check_pipeline` + `state::check_learned`
- `coursers post` internally calls `chain.run_post()` instead of direct
  `state::record_failure`
- External interface unchanged: stdin JSON in, stdout JSON out, same
  exit codes

### Phase E: Merge binaries

- Single `coursers` binary handles both `pre` and `post` subcommands
  with shared chain construction
- `crs rewrite` and `crs filter` logic absorbed into the chain
- `settings.json` simplified: one hook entry per event instead of
  multiple binaries
- TODO(joe::JOB-475): composition root lives in main.rs — no business logic there

### Phase F (future): Persistent process

- `coursers serve` — long-lived process, unix socket or stdin/stdout
  multiplexing
- Chain stays warm: compiled regexes, loaded state, no cold-start
- Requires Claude Code protocol support for persistent hook processes

## Dependency on other plans

- **Phase 4 of godmode-integration** (task-aware overrides) becomes
  `TaskOverride` hook in the chain — cleaner than the standalone
  implementation described in that plan
- **Shared trait extraction** (devkit plan) — the `HookChain` compositor
  could live in devkit if other projects adopt the same pattern

## Files touched

| Phase | Files                                                                                                                                                                                           |
| ----- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A     | `crs-core/src/hook/chain.rs`, `crs-core/src/hook/decision.rs`, `crs-core/src/hook/mod.rs`                                                                                                       |
| B     | `crs-core/src/hook/blocker.rs`, `crs-core/src/hook/learner.rs`, `crs-core/src/hook/swapper.rs`, `crs-core/src/hook/filter.rs`, `crs-core/src/hook/recorder.rs`, `crs-core/src/hook/redactor.rs` |
| C     | `crs-core/src/config.rs` (extend), new `AppConfig`                                                                                                                                              |
| D     | `crates/coursers/src/hook/pre.rs`, `crates/coursers/src/hook/post.rs`                                                                                                                           |
| E     | `crates/coursers/src/main.rs`, `crates/crs/src/main.rs`, remove redundant entry points                                                                                                          |
| F     | New `crates/coursers/src/serve.rs`                                                                                                                                                              |

## Acceptance criteria

- [ ] All existing tests pass with chain-based internals
- [ ] `coursers pre` + `coursers post` produce identical stdin/stdout
      behavior (backward compatible)
- [ ] Single config load per invocation (measurable via tracing)
- [ ] Hook ordering is explicit in code, not in settings.json
- [ ] New hooks can be added by implementing `PreHook`/`PostHook` and
      adding one line to `build_chain()`
- [ ] Benchmark: chain evaluation <3ms for 20 rules (down from ~4ms
      for two separate processes)
