# Hook Pipeline Architecture

## Overview

The coursers workspace now has a generic hook pipeline (`crs-core/src/hook/pipeline.rs`) that handles all Claude Code hook events through declarative TOML rules.

## Key Types

- **HookEvent** — PreToolUse, PostToolUse, SessionStart, SessionEnd, PreCompact, Stop, SubagentStop
- **HookAction** — Deny (block with message), Rewrite (modify command), Run (side-effect), Notify (emit info)
- **HookRule** — event + matcher + pattern + unless + when + action
- **HookContext** — runtime data passed to the pipeline (tool_name, target, exit_code)

## Config Hierarchy

1. `~/.config/crs/hooks.toml` — global base
2. `~/.config/crs/plugins.d/*.toml` — plugins (godmode, etc.)
3. `.ctx/crs-hooks.toml` — project-local overrides

## Entry Point

`crs hook <event>` reads stdin JSON, builds HookContext, runs pipeline, emits response.

## godmode Plugin

`~/.config/crs/plugins.d/godmode.toml` contains 20 rules replacing 9 nu scripts:

- 7 destructive-guardian deny rules
- 2 pre-tool rewrites (nextest-op-wrap, doob-branch-tagger)
- 1 pre-commit handoff side-effect
- 5 generated-file edit guards
- 3 post-tool side-effects (doob autocomplete, gh-sync, snap-notify)
- 1 lifecycle hook (stop → hj handoff)

## Wiring

settings.json uses `crs hook pre-tool-use` and `crs hook post-tool-use` as catch-all
entries, with `coursers pre/post` and `crs rewrite/filter` retained for their
specialized state (rule matching, failure learning, output compression, rx prefix).
