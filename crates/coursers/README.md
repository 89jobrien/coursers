# coursers

The `coursers` package is the executable and protocol-adapter layer for the Coursers workspace. It
ships two binaries, `coursers` and `crs`, over one shared Clap model and dispatch function. The
package connects `coursers-core` policy to Claude Code, Codex, and OpenCode hook protocols.

The workspace policy requires Rust 1.89 or newer and uses Rust 2024. This package inherits the
edition, but its manifest currently does not inherit the workspace `rust-version`. It is licensed
under MIT OR Apache-2.0.

## Contents

- [Workspace role](#workspace-role)
- [Install and inspect](#install-and-inspect)
- [Command reference](#command-reference)
- [Static course-correction rules](#static-course-correction-rules)
- [Filters, rewrites, and tool swaps](#filters-rewrites-and-tool-swaps)
- [Generic hook pipeline](#generic-hook-pipeline)
- [Harness compatibility](#harness-compatibility)
- [Safety contracts](#safety-contracts)
- [Library API](#library-api)
- [Development and testing](#development-and-testing)

## Workspace role

This crate owns:

- CLI parsing and command presentation.
- stdin/stdout hook protocol handling and harness-specific exit behavior.
- concrete RTK and obfsck subprocess clients.
- Claude, Codex, and OpenCode routing and installation validation.
- Nushell syntax checking through `nu --ide-check`.

Rule, state, parser, filter, rewrite, analysis, and hook-composition algorithms belong in
`coursers-core`. Shared data and stable ports belong in `coursers-types`.

## Install and inspect

From the workspace root:

```sh
cargo install --path crates/coursers
coursers --help
crs --help
```

Both names expose the same documented subcommands and call `coursers::run`. `coursers` alone
intercepts `coursers completions` before Clap parsing and writes Nushell completions to stdout;
`crs` does not provide that entrypoint shortcut. Building or testing the workspace does not update
binaries already installed under `$HOME/.cargo/bin`; use `just install` before validating live
hooks.

## Command reference

| Command | Purpose and important options |
| --- | --- |
| `pre` | Read `PreToolUse` JSON; `--profile`, `--rules`, and `--state` select config |
| `post` | Read `PostToolUse` JSON and record eligible failures; same config options |
| `filter` | Suppress or replace PostToolUse output from filter rules |
| `rewrite` | Rewrite a PreToolUse command; exit 1 means unchanged |
| `discover` | Scan history; includes limits, formats, `--min-count`, and `--generate-filters` |
| `validate` | Validate static rule regexes, examples, exceptions, and alternatives |
| `probe` | Read a raw command or hook JSON from stdin and explain rule decisions |
| `stats` | Show cumulative blocks per rule |
| `insights` | Enrich captured facets with git context; `--format`, `--since`, `--repo` |
| `audit` | Show rx prefix-learning state or remove one entry with `--remove` |
| `suggest` | Suggest rules from unhandled history; supports history and format limits |
| `history` | Show recent blocks; filter with `--rule` and select `--format` |
| `export` | Write rules, stats, and state as JSON to stdout or `--out` |
| `hook` | Run an event through `--target claude`, `codex`, or `opencode` |
| `validate-hooks` | Validate the selected harness installation or generic hook rules |
| `log` | Query by event/outcome, format output, or prune with `--prune-hours` |
| `heat` | Render rule-firing heat data, optionally restricted by `--rule` |
| `replay` | Replay one session or an inferred recent session through current rules |
| `nu-check` | Check files, `$HOME/.claude/hooks/nu`, or nu libraries with `nu --ide-check` |

Run `crs <command> --help` for exact options. Formats are accepted as strings and validated by the
handler; commonly supported values are `text` and `json`.

The current `filter` and `rewrite` help includes profile/rules options, and `stats` accepts a
profile, but those handlers do not yet consume the resolved profile. Use `CRS_FILTERS` for an
explicit filter/rewrite file and do not rely on those flags until the profile-aware CLI work lands.

## Static course-correction rules

`pre`, `post`, `validate`, and `probe` use a rules JSON file. Resolution is:

1. Explicit `--rules` for commands that support it.
2. A named `$HOME/.config/coursers/profiles/<name>/rules.json`.
3. `COURSERS_RULES` for the default profile.
4. `$HOME/.config/coursers/course-correct-rules.json`.

A minimal file is:

```json
{
  "rules": [
    {
      "id": "no-grep",
      "pattern": "\\bgrep\\b",
      "target_commands": ["grep"],
      "exceptions": ["\\|\\s*grep"],
      "message": "Use the Grep tool instead."
    }
  ],
  "failure_learning": {
    "enabled": true,
    "block_threshold": 3,
    "window_seconds": 300,
    "max_tracked_commands": 200,
    "cleanup_after_seconds": 3600
  }
}
```

Rules are evaluated in order. A rule must be enabled, pass its optional target-command gate, match
its regex, and match no exception. `pattern_flags` supports `i`. `task_override` can suspend one
rule while a matching godmode task is running, but later matching rules are still checked.

Failure-learning state resolves from explicit/profile settings, `COURSERS_STATE`, an existing local
`.ctx/course-correct-state.json`, then the global Coursers config directory. Successful commands,
signals, and known intentional-failure forms are not learned.

Probe a command without executing it:

```sh
printf '%s\n' 'grep TODO .' | crs probe
```

`pre` is pipeline-aware and can differ from `probe`: the real hook path checks sequential command
segments and then the whole command, while probe reports whole-input rule behavior. Reproduce hook
issues with the exact payload and compare both paths.

## Filters, rewrites, and tool swaps

`filter` and `rewrite` read TOML from `CRS_FILTERS`, project `.ctx/crs-filters.toml`, and global
`$HOME/.config/crs/filters.toml`. The environment variable is exclusive; otherwise project content
precedes global content.

```toml
[[filters]]
pattern = "cargo nextest"
mode = "failures-only"

[[rewrites]]
pattern = "^cargo test(.*)"
replace = "cargo nextest run$1"

[tool_swap]
cat_token_limit = 4000
tail_limit_max = 500
find_depth_max = 10
```

Filter modes are `passthrough`, `failures-only`, `errors-only`, `truncate`, and `match-lines`.
Failures are preserved by the modes intended to compress successful output. Rewrites run in file
order and can cascade. A successful rewrite emits protocol JSON; no change exits 1 so the harness
can continue unchanged.

The opt-in HookChain uses profile-backed adapters. Its filter adapter selects the project file when
present rather than supplementing it with global filters; its rewrite adapter still merges project
and global rules. Legacy `crs filter` and `crs rewrite` merge both files.

## Generic hook pipeline

The preferred Claude front controller is one command per event:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [{"type": "command", "command": "crs hook pre-tool-use"}]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Bash",
        "hooks": [{"type": "command", "command": "crs hook post-tool-use"}]
      }
    ]
  }
}
```

The Claude front controller runs static course-correction rules first for `PreToolUse` Bash events,
then evaluates generic TOML rules loaded from:

```text
$HOME/.config/crs/hooks.toml
$HOME/.config/crs/plugins.d/*.toml
.ctx/crs-hooks.toml
```

Supported event names are `pre-tool-use`, `post-tool-use`, `session-start`, `session-end`,
`permission-request`, `pre-compact`, `post-compact`, `user-prompt-submit`, `subagent-start`, `stop`,
and `subagent-stop`.

A generic rule can deny, rewrite, run a process, notify, or redact output:

```toml
[[hooks]]
event = "pre-tool-use"
matcher = "Bash"
pattern = "^rm -rf"
unless = "target/"
action = "deny"
message = "Refusing an unscoped recursive deletion."
label = "safety/rm-rf"
```

`unless` is a regex in the runtime. `when = "on-success"` or `"on-failure"` gates post-tool rules.
`run` actions are real side effects, not dry runs. Project-local TOML must therefore be treated as
trusted executable configuration. Run `crs validate-hooks` after changing pipeline files.

## Harness compatibility

### Claude Code

Claude deny responses use structured JSON on stdout and exit code 2. Allows are normally silent.
Rewrites return `permissionDecision: "allow"` plus `updatedInput.command`. Diagnostics must remain
on stderr because stdout is the hook protocol channel.

### Codex

`crs hook --target codex <event>` selects a backend under `$HOME/.codex/hooks/*.crux` and invokes it
through `crux run`. `validate-hooks --target codex` checks the registry and availability of
`coursers`, `crs`, and `crux`. Backend stdout, stderr, and nonzero status are propagated.

### OpenCode

Install both `integrations/opencode/coursers.ts` and `opencode-plugin.d.ts` in project-local
`.opencode/plugins/` or global `$HOME/.config/opencode/plugins/`. The adapter returns neutral JSON
with `decision`, `reason`, `updated_input`, `replacement_output`, messages, and matched rules.
OpenCode deny decisions intentionally use exit code 0 so the plugin can consume the structured
decision. Validate with:

```sh
crs validate-hooks --target opencode
bun test integrations/opencode/coursers.test.ts
```

## Safety contracts

- Never mix diagnostics with protocol JSON on stdout.
- Static and optional filter configuration generally fail open: warn and preserve the tool call or
  output unless a protocol explicitly requires rejection.
- Never print secrets from hook payloads, process environments, logs, or configuration.
- Treat generic `run` actions and Codex backends as executable code.
- Query actual behavior with `crs log`; avoid replaying sensitive sessions into shared output.
- `COURSERS_HOOK_CHAIN=1` opts `pre` and `post` into the composable chain. The legacy path remains
  the default, and E2E tests assert representative parity.

## Library API

The package also exposes `Cli`, `Command`, `build_profile`, and `run` for thin entrypoints and test
harnesses. `hook::pre::run_with` and `hook::post::run_with` accept injected loaders/stores. Concrete
`ProcessRtkClient` and `ProcessObfsckMcpClient` adapters are available through their modules. Most
consumers should invoke the binaries rather than depend on this outer adapter crate as a library.

## Development and testing

```sh
cargo check -p coursers
cargo clippy -p coursers --all-targets --all-features -- -D warnings
cargo nextest run -p coursers
cargo nextest run -p coursers-e2e
bun test integrations/opencode/coursers.test.ts
nu scripts/smoke.nu
```

Binary integration tests isolate HOME, rule, state, and filter paths with temporary directories and
assert status, stdout JSON, and stderr independently. Use `just install` before tests that exercise
the user's live hook registry or installed binaries.
