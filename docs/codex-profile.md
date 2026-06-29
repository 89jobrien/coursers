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

Checks `~/.codex/hooks.json` for the 4 expected hook commands and
verifies `coursers` and `crs` binaries are on PATH.
