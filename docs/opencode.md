# OpenCode Harness Support

Coursers integrates with OpenCode through a TypeScript plugin that translates OpenCode events
into the harness-neutral `crs hook --target opencode <event>` JSON protocol.

## Requirements

- OpenCode 1.2.26 or a compatible plugin API
- Bun, as provided by the OpenCode plugin runtime
- `crs` and `opencode` on `PATH`
- A built or installed Coursers binary

## Installation

The repository ships `integrations/opencode/coursers.ts` with the compile-time compatibility
subset `integrations/opencode/opencode-plugin.d.ts`. Copy or symlink exactly those two files
into one of these directories; do not copy `coursers.test.ts` or the whole integration directory:

- Project: `.opencode/plugins/coursers.ts` and `.opencode/plugins/opencode-plugin.d.ts`
- Global: `$HOME/.config/opencode/plugins/coursers.ts` and
  `$HOME/.config/opencode/plugins/opencode-plugin.d.ts`

The declaration file mirrors the OpenCode 1.2.26 plugin and SDK fields used by Coursers. It adds
no runtime dependency and must remain beside `coursers.ts` for standalone TypeScript checks.
Project installation takes precedence during validation. Coursers never writes OpenCode global
configuration automatically.

After installation, run:

```sh
crs validate-hooks --target opencode
```

## Event Mapping

| Coursers event | OpenCode source | Application behavior |
| --- | --- | --- |
| `PreToolUse` | `tool.execute.before` | Deny by throwing; apply rewrites to `output.args` |
| `PostToolUse` | `tool.execute.after` | Replace `output.output` when filtering or redaction fires |
| `UserPromptSubmit` | `chat.message` | Deny by throwing; log non-blocking messages |
| `PermissionRequest` | `permission.ask` | Set `output.status` to `deny` on denial |
| `PreCompact` | `experimental.session.compacting` | Run before compaction prompt generation |
| `PostCompact` | `session.compacted` | Run after successful compaction |
| `SessionStart` | Root `session.created` | Run once for a session without `parentID` |
| `SubagentStart` | Child `session.created` | Run once for a session with `parentID` |
| `Stop` | Root `session.idle` | Run when a root session becomes idle |
| `SubagentStop` | Child `session.idle` | Run when a child session becomes idle |
| `SessionEnd` | `session.deleted`, plus tracked roots at plugin disposal | Run once per tracked root using in-memory deduplication |

`session.idle` maps to `Stop`, not `SessionEnd`, because idle sessions remain resumable.
Duplicate idle events are suppressed until an explicit `session.status` event reports `busy`,
which rearms stop delivery for the next busy-to-idle cycle. Session relationships and end-event
deduplication are held in memory. `SessionEnd` delivery is
best effort when OpenCode emits `session.deleted` or `server.instance.disposed` during plugin
disposal.

Tool names are normalized to Coursers names: `bash` becomes `Bash`, `edit` becomes `Edit`, and
`write` becomes `Write`. The plugin also converts `filePath` to `file_path`.

## Neutral JSON Contract

The plugin sends normalized JSON on stdin. A Bash tool request resembles:

```json
{
  "tool_name": "Bash",
  "tool_input": { "command": "cargo test" },
  "tool_response": { "exit_code": 0, "output": "ok" },
  "session_id": "ses-root"
}
```

The CLI writes exactly one response object:

```json
{
  "decision": "allow",
  "reason": null,
  "updated_input": null,
  "replacement_output": null,
  "messages": [],
  "matched_rules": []
}
```

A denial uses `decision: "deny"` and a reason while still exiting zero. Rewrites populate
`updated_input`; filters and redaction populate `replacement_output`.

## Failure Semantics

Invalid normalized JSON or an unknown event makes `crs` exit non-zero and write diagnostics to
stderr. The plugin logs adapter failures and fails open. A valid structured denial is still
applied. Missing or non-numeric `metadata.exit` defaults to zero, preventing unverified failure
records. This may omit failure learning when a tool does not provide exit metadata.

Notifications are written to OpenCode application logs at `info` level; they are not injected
into model context. Lifecycle hooks that cannot block their originating event still perform
configured side effects.

## Validation

`crs validate-hooks --target opencode` checks, without executing hooks or changing
configuration:

1. `coursers.ts` and `opencode-plugin.d.ts` together in `.opencode/plugins/`
2. Both files together in `$HOME/.config/opencode/plugins/` as a fallback
3. Availability of `crs` and `opencode` on `PATH`

The command exits non-zero and identifies every missing requirement.

## Troubleshooting

- **Plugin not found:** verify `coursers.ts` and `opencode-plugin.d.ts` are together under one
  supported plugin directory and run validation from the intended project.
- **Bridge errors in logs:** run `crs hook --target opencode session-start` with a JSON object on
  stdin and inspect stderr. Adapter failures fail open by design.
- **No failure-learning record:** confirm the OpenCode tool supplied numeric `metadata.exit`.
  Missing values intentionally default to zero.
- **Repeated or missing end events:** relationship and deduplication state is process-local;
  abrupt plugin termination can prevent best-effort `SessionEnd` delivery.
- **Compaction hook unavailable:** `experimental.session.compacting` is version-sensitive; other
  hooks continue to operate if OpenCode does not expose it.
