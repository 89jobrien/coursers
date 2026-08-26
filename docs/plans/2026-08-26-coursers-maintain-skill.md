# Plan: Coursers Maintenance Skill

## Goal

Replace the stale Coursers companion with one accurate skill for repository development
and safe `coursers`/`crs` operation.

## Context Map

### Files to Modify

| File | Purpose | Changes Needed |
| --- | --- | --- |
| `$HOME/.agents/skills/coursers-maintain/SKILL.md` | Active agent skill | Create the authoritative project companion |
| `.codex/skills/coursers-maintain/SKILL.md` | Tracked Codex project skill | Replace stale content with the authoritative skill |

### Dependencies

| File | Relationship |
| --- | --- |
| `AGENTS.md` | Live repository workflow and architecture source |
| `CLAUDE.md` | Live command, configuration, and gotcha source |
| `Cargo.toml` | Workspace membership source |
| `crates/coursers/Cargo.toml` | Binary ownership source |
| `.codex/skills/coursers-maintain/agents/openai.yaml` | Existing Codex metadata; no change expected |

### Reference Patterns

| File | Pattern to Follow |
| --- | --- |
| `$HOME/.agents/skills/notfiles/SKILL.md` | Broad Rust project companion with operational safety |
| `$HOME/.agents/skills/nu-libs/SKILL.md` | Concise repository-specific navigation skill |
| `$HOME/.claude/skills/crs-hook-testing/SKILL.md` | Narrow specialist skill that retains hook simulation ownership |

### Risk

- No Rust API, serialization, CLI output, or crate dependency changes.
- The main risks are stale architecture, overlapping triggers, and drift between copies.

## Architecture

- Crates affected: none.
- New traits/types: none.
- Data flow: active `SKILL.md` content -> synchronized Codex copy -> agent trigger and use.

## Tech Stack

- Agent Skill Markdown with YAML frontmatter.
- No new dependencies, scripts, or generated assets.

## Tasks

### Task 1: Author the active skill

**File(s)**: `$HOME/.agents/skills/coursers-maintain/SKILL.md`

1. Create the skill directory under the existing active `$HOME/.agents/skills/` tree.
2. Write frontmatter with the exact name `coursers-maintain` and a trigger-focused
   description covering Coursers development, hook behavior, configuration, and CLI use.
3. Add the approved body sections: orientation, workspace map, ownership guide,
   development workflow, CLI operations, hook debugging handoff, and known gotchas.
4. Keep release orchestration, generic Rust guidance, and detailed hook simulation out of
   scope.
5. Read the completed file and verify every architecture statement against the context
   files listed above.

### Task 2: Replace and validate the Codex project copy

**File(s)**: `.codex/skills/coursers-maintain/SKILL.md`

1. Replace the stale Codex skill body with the authoritative active skill content.
2. Confirm `.codex/skills/coursers-maintain/agents/openai.yaml` remains compatible; leave
   it unchanged unless its prompt contradicts the replacement skill.
3. Verify the two `SKILL.md` files are byte-for-byte identical.
4. Validate the skill frontmatter and scan for stale `crates/crs`, `crs-core`, or
   OpenKnowledge references.
5. Check positive trigger examples for Coursers development and CLI operation, plus
   negative examples for generic Rust and unrelated release work.

## Verification

- Active and tracked skill files match exactly.
- Frontmatter contains only the expected `name` and `description` fields.
- Workspace map lists five crates and identifies `coursers` and `crs` as binaries from
  the `coursers` package.
- `crs-hook-testing` remains the owner of detailed hook-rule simulation.
- `git diff` contains no unrelated user-owned HANDOFF changes.
