# Handoff — coursers (2026-04-06)

**Branch:** main | **Build:** ok | **Tests:** 80 passed, 0 failed

## Items

| ID | P | Status | Title |
|---|---|---|---|
| coursers-9 | P1 | open | Migrate config defaults from ~/.claude/hooks/ to ~/.config/coursers/ |
| coursers-10 | P1 | open | RTK integration — RtkAnalysis/RtkRewrite ports + ProcessRtkClient adapter |
| coursers-5 | P2 | open | Wire `crs rewrite` and `crs filter` into settings.json |
| coursers-11 | P2 | open | Build obfsck MCP server + mcpipe integration for custom filter generation |
| coursers-1 | P1 | done | Implement `crs filter` PostToolUse hook — output compression |
| coursers-2 | P1 | done | Implement `crs rewrite` PreToolUse hook — optional command rewriting |
| coursers-3 | P1 | done | Define crs-filters.toml schema and config loader |
| coursers-4 | P2 | done | Implement `crs discover` — scan Claude Code history for missed savings |
| coursers-6 | P2 | done | Create coursers Claude Code plugin with companion agent |
| coursers-7 | P2 | done | Add `crs validate` — rule health check against available commands |
| coursers-8 | P2 | done | Add `crs probe` — interactive rule inspection |

## Log

- 2026-04-06: Built RTK port/adapter layer, obfsck audit integration, coursers-10/11 added
- 2026-04-06: Scoped state file to .ctx/, cleaned .gitignore, identified coursers-9 XDG migration
- 2026-04-06: Added crs validate, fixed no-grep-use-tool exceptions, added crs probe, fixed
  discover rule attribution, switched to real token counts from session output_bytes.
  [032973b, 1948e1e, b33548a, 65a49da]
- 2026-04-06: Implemented crs discover with hexagonal CommandSource architecture, real JSONL
  parsing, frequency grouping, savings report. [457e9e1, b36548e, e9ea857]
- 2026-04-06: Built full test harness, extracted traits, 64 tests. Wrote README.
  [50aa03c, b973f89, 11d93c4]
- 2026-04-06: Implemented all P1 items. 46 tests passing. [f151666]
- 2026-04-06: Converted to workspace, wired hooks, created plugin. [ca7ffdc]
