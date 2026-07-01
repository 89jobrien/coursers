# Codex Profile — Rules Divergence

The Codex profile (`--profile codex`) uses a reduced rule set and
different hook protocol compared to the default (Claude) profile.

## Protocol Differences

| Aspect     | Claude Code            | Codex                               |
| ---------- | ---------------------- | ----------------------------------- |
| Deny exit  | exit 2                 | exit 0 + JSON `deny` decision       |
| Output key | `tool_response.output` | `tool_response.stdout` (+ `output`) |

## Rule Differences (base=15, codex=13)

| Rule                          | Base                              | Codex                                          |
| ----------------------------- | --------------------------------- | ---------------------------------------------- |
| `no-sed-n-use-read`           | enabled                           | disabled (stub)                                |
| `no-grep-use-tool`            | 9 exceptions (incl `-c` patterns) | 7 exceptions                                   |
| `no-sleep-find-work`          | `sleep` only                      | `sleep\|timeout`, 3 timeout exceptions         |
| `no-bash-use-nu`              | 1 exception (`^nu\b`)             | 3 exceptions (git commit heredoc, gh api --jq) |
| `no-python3-file-edit`        | present                           | absent                                         |
| `no-cargo-install-multi-path` | present                           | absent                                         |

## Validation

```sh
crs validate-hooks --target codex
```

Checks `~/.codex/hooks.json` for the full verified Codex hook registry and
verifies `coursers`, `crs`, and `crux` are on PATH.

The expected top-level commands are:

- `SessionStart` (`startup|resume`) → `crs hook --target codex session-start`
- `PreToolUse` (`Bash`) → `crs hook --target codex pre-tool-use`
- `PostToolUse` (`Bash`) → `crs hook --target codex post-tool-use`
- `PostToolUse` (`Edit|Write`) → `crs hook --target codex post-tool-use`
- `PermissionRequest` → `crs hook --target codex permission-request`
- `PreCompact` → `crs hook --target codex pre-compact`
- `PostCompact` → `crs hook --target codex post-compact`
- `UserPromptSubmit` → `crs hook --target codex user-prompt-submit`
- `SubagentStart` → `crs hook --target codex subagent-start`
- `SubagentStop` → `crs hook --target codex subagent-stop`
- `Stop` → `crs hook --target codex stop`

Those front-controller entries fan back into the existing
`$HOME/.codex/hooks/*.crux` backends.
