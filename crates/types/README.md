# coursers-types

`coursers-types` defines the dependency-light domain data and port traits shared by the Coursers
workspace. It is the innermost architectural crate: policy and adapters may depend on these
contracts, while this crate does not depend on `coursers-core` or the CLI package.

The workspace policy requires Rust 1.89 or newer and uses Rust 2024. This package inherits the
edition, but its manifest currently does not inherit the workspace `rust-version`. It is licensed
under MIT OR Apache-2.0.

## Workspace role

Use this crate for stable serialized shapes and hexagonal ports. Put algorithms, filesystem access,
process execution, and command presentation in outer crates.

| Module | Contracts |
| --- | --- |
| `capture` | Suggestion records, construction parameters, and deduplication keys |
| `config` | Token estimate constant, hook protocol, and resolved profile paths |
| `filters` | Filter, rewrite, and tool-swap configuration and filter outcomes |
| `history` | Session command records and discovery options |
| `hook` | Claude-compatible hook request and pre-hook response envelopes |
| `obfsck` | Secret-audit hits and generated filter suggestions |
| `pipeline` | Generic hook events, rules, actions, contexts, results, and diagnostics |
| `ports` | Traits for rules, state, capture, history, stats, filters, RTK, and tool data |
| `rtk` | RTK analysis, savings, verification, and probe report shapes |
| `rules` | Static block rules and failure-learning configuration |
| `state` | Persisted rolling failure state |
| `stats` | Per-rule block counters and last-seen timestamps |

`coursers-core` still exposes compatibility traits and re-exports while contracts are being moved
inward. New boundary code should prefer `coursers_types::ports` when its associated-error contract
fits the adapter.

## Using the crate

From another workspace package:

```toml
[dependencies]
coursers-types = { path = "../types" }
```

An adapter chooses its own error type through each port's associated `Error`:

```rust
use coursers_types::ports::rules::RulesLoader;
use coursers_types::rules::RulesConfig;

struct EmptyRules;

impl RulesLoader for EmptyRules {
    type Error = std::convert::Infallible;

    fn load(&self) -> Result<RulesConfig, Self::Error> {
        Ok(RulesConfig::default())
    }
}
```

The `core-compat` feature enables `FiltersConfig::load_from`. That helper performs filesystem and
TOML parsing for compatibility with `coursers-core`; without the feature, consumers can keep I/O in
their own adapters.

## Key contracts

### Static rules and failure learning

`rules::Rule` describes a regex rule. `enabled` defaults to `true`, `pattern_flags` currently
recognizes `i` in core, and any matching exception skips the rule. `target_commands` can restrict a
rule to pipeline stages whose executable name matches. `task_override` supports an exact title or a
single trailing `*` convention interpreted by core.

`rules::FailureLearning` defaults to three failures in 300 seconds, retains at most 200 commands,
and removes stale entries after 3600 seconds. Additive serialized fields use Serde defaults so old
configuration and state files remain readable.

### Filter and rewrite configuration

`filters::FilterMode` is serialized in kebab case and supports:

- `passthrough`: preserve output.
- `failures-only`: suppress successful output and preserve failures.
- `errors-only`: retain lines containing `error`, case-insensitively.
- `truncate`: retain at most `max_lines`, which defaults to 50.
- `match-lines`: retain regex-matching lines on success and fail open for invalid expressions.

`RewriteRule` contains a regex `pattern` and replacement string. Core applies matching rewrite rules
in file order to the current command, so later rules can act on earlier output.

### Hook protocol shapes

`hook::HookPayload` accepts the common Claude `PreToolUse` and `PostToolUse` fields. Optional fields
allow non-Bash or partial harness events to pass through without deserialization failures.
`PreResponse` preserves the harness's camel-case JSON keys through explicit Serde renames.

`config::HookProtocol` records a material compatibility difference: Claude denies with exit code 2,
whereas Codex returns a deny decision as JSON with exit code 0. Formatting and exit behavior belong
to outer protocol adapters, not these data types.

### Generic pipeline

`pipeline::HookEvent` covers tool, lifecycle, compaction, permission, prompt, and subagent events.
Rules combine an event, optional tool matcher, regex pattern, exception, timing condition, action,
and label. The types crate currently models `deny`, `rewrite`, `run`, and `notify`; check
`coursers-core` before extending the runtime because compatibility pipeline types still exist there.

## Compatibility and safety

- Treat public serialized fields as protocol contracts; add fields with defaults rather than
  renaming or removing existing fields.
- Port traits contain no concrete filesystem or process implementation.
- `CommandSource::commands` returns an iterator, allowing history adapters to stream records.
- `CaptureStore`, `StateStore`, and `StatsStore` make mutation explicit through `Result` values.
- Hook payloads and persisted state may contain commands or paths; do not log them without applying
  the outer crate's redaction policy.
- The canonical contract surfaces are tracked by `taskit.toml`; intentional changes require the
  repository's protocol-drift workflow.

## Development and testing

From the workspace root:

```sh
cargo check -p coursers-types
cargo clippy -p coursers-types --all-targets --all-features -- -D warnings
cargo nextest run -p coursers-types
cargo xtask protocol drift
```

Unit tests cover defaults, Serde compatibility, and representative filter/rewrite configuration.
Doctests are disabled in the manifest, so examples in this README are illustrative compile targets,
not part of the package test suite.
