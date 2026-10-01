# coursers-e2e

`coursers-e2e` owns black-box tests for complete Coursers command and hook flows. It launches the
workspace-built `coursers` and `crs` binaries, supplies isolated configuration and state, and checks
behavior across process boundaries.

This package is test infrastructure only (`publish = false`). Its `e2e-placeholder` binary is empty;
the useful behavior lives in integration tests under `tests/`.

## Workspace role

Use this crate when a behavior crosses package or protocol boundaries, including:

- static rules flowing through `coursers pre`;
- failures recorded by `post` and enforced by a later `pre`;
- rewrite and filter configuration consumed by `crs`;
- the `crs hook pre-tool-use` front-controller path;
- parity between the legacy pre/post implementation and `COURSERS_HOOK_CHAIN=1`.

Focused domain behavior belongs in `coursers-core` tests. Binary-specific flags and response shapes
belong in `crates/coursers/tests`. This package proves that the assembled workspace works as a user
would invoke it.

## Test suites

| Target | Coverage |
| --- | --- |
| `scenarios` | Data-driven static blocks, allows, filters, and learned failures |
| `pipeline` | Hand-written rewrite/pre/post/filter flows and front-controller regression cases |
| `hook_chain` | Legacy versus opt-in HookChain outcomes for block, rewrite, filter, and learning |

The harness creates temporary rule, filter, HOME, and state locations. It does not depend on the
user's live configuration.

## Running the tests

Build the release binaries first because the harness prefers `target/release` over `target/debug`:

```sh
cargo build --workspace --release
cargo nextest run -p coursers-e2e
```

Run one suite or one named test:

```sh
cargo nextest run -p coursers-e2e --test scenarios
cargo nextest run -p coursers-e2e --test pipeline \
  -E 'test(crs_hook_pre_tool_use_runs_course_correct_rules)'
```

To test debug binaries instead, remove stale `target/release/coursers` and `target/release/crs`
before running `cargo build --workspace`. The helper has no binary-path override; while either
release binary exists, it is selected instead of the corresponding debug binary.

## Scenario fixtures

Files under `fixtures/scenarios/*.toml` use this shape:

```toml
[scenario]
name = "grep-blocked"
description = "grep is blocked by coursers pre"
coursers_rules_json = '''
{"rules":[{"id":"no-grep","pattern":"\\bgrep\\b","message":"Use Grep."}]}
'''
crs_filters_toml = """
[[filters]]
pattern = "cargo nextest"
mode = "failures-only"
"""

[pre]
command = "grep TODO ."
expected_verdict = "block"

[filter]
command = "cargo nextest run"
output = "42 tests passed"
exit_code = 0
expected_result = "suppress"

[post_failures]
command = "unique-failing-command"
count = 3
```

All phases are optional. Supported expectations are:

- `pre.expected_verdict`: `allow` or `block`.
- `filter.expected_result`: `passthrough`, `replace`, or `suppress`.
- `post_failures.count`: number of nonzero post events sent before the pre assertion.

The scenario runner sorts fixture paths for deterministic execution. Use a unique command string
for failure-learning scenarios, even though state is tempdir-isolated, to make failures easy to
diagnose.

## Protocol and safety contracts

- A static Claude deny must be nonzero and include a deny/block response; current tests expect exit
  code 2 where they assert the exact protocol.
- An allow is exit 0 and normally silent.
- Filtering is a post-tool allow response: suppression is represented by an empty replacement
  message, not a denied tool call.
- Rewriting is an allow response carrying `hookSpecificOutput.updatedInput.command`.
- Failure-learning writes only to the fixture state path supplied through `COURSERS_STATE`.
- `COURSERS_HOOK_CHAIN=1` must preserve representative legacy outcomes before it can become default.
- Tests must never read or write `$HOME/.config/coursers`, `$HOME/.config/crs`, or live hook logs.

## Extending coverage

Add a fixture when a case is expressible as one linear rule/filter scenario. Add Rust integration
code when the test needs multiple invocations, exact JSON inspection, environment switching, or
legacy/chain comparison. Assert exit status, stdout, and stderr separately, and keep every test's
state in its own `TempDir`.

Before submitting E2E changes, run:

```sh
cargo fmt --all -- --check
cargo clippy -p coursers-e2e --all-targets -- -D warnings
cargo build --workspace
cargo nextest run -p coursers-e2e
```
