# Design: Coursers Maintenance Skill

## Goal

Replace the stale `coursers-maintain` projection with one authoritative project-scoped
skill for both repository development and safe `coursers`/`crs` operation.

## Approved Approach

Use the **Unified Project Companion** approach: one concise skill with broad project
triggers and explicit handoffs to narrower specialist skills.

## Ownership

- **Owner**: active agent skill at
  `$HOME/.agents/skills/coursers-maintain/SKILL.md`.
- **Affected project copy**: the existing
  `.codex/skills/coursers-maintain/SKILL.md` remains synchronized for Codex project use.
- **Rust crates**: none; this change does not modify workspace code or public APIs.

## Skill Contract

### Name

`coursers-maintain`

### Trigger Scope

Load the skill for development, debugging, configuration, documentation, validation,
or practical CLI usage specific to the `coursers` repository and its `coursers`/`crs`
binaries.

Do not load it for generic Rust questions or release orchestration that does not require
project-specific context.

### Body Structure

1. Orientation and precedence of live project documentation.
2. Accurate workspace and binary map.
3. Task-to-crate ownership guidance.
4. Development and validation workflow.
5. CLI configuration and operational safety.
6. Hook debugging workflow and specialist-skill handoffs.
7. Known project gotchas, including reinstalling binaries before live verification.

Keep the body self-contained and concise. Add no reference or script files unless the
body becomes too large to follow reliably.

## Data Flow

1. **Source**: `$HOME/.agents/skills/coursers-maintain/SKILL.md` is the authoritative
   agent skill.
2. **Project compatibility**: the tracked Codex skill copy uses the same frontmatter and
   body.
3. **Sink**: agents load `coursers-maintain` when its description matches a
   Coursers-specific task.

## Integration Boundaries

- Read live `AGENTS.md`, `CLAUDE.md`, manifests, and nearby implementation before relying
  on cached architectural details in the skill.
- Delegate hook-rule simulation details to `crs-hook-testing` rather than duplicating its
  procedure.
- Delegate generic Rust, release, debugging, and verification disciplines to their
  existing specialist skills when applicable.
- Preserve the existing Codex agent metadata and update its prompt only if needed to match
  the replacement skill.

## Validation

- Validate the skill name and frontmatter contract.
- Verify crate ownership and both binary entry points against current manifests.
- Check that trigger examples activate for Coursers-specific work and avoid generic Rust
  work.
- Confirm `crs-hook-testing` remains the narrower owner of hook-rule simulation.
- Confirm the active `$HOME/.agents/skills/` tree and tracked Codex copy contain matching
  skill content.

## Out of Scope

- Rust source or public API changes.
- Release orchestration guidance.
- New helper scripts or executable skill assets.
- Duplicating generic Rust conventions or the detailed `crs-hook-testing` workflow.

## Risk

- [x] Breaking API changes: no.
- [x] New external dependency: no.
- [x] Feature flag required: no.
- [ ] Copy drift: mitigated by verifying the active skill and tracked Codex copy match.
- [ ] Trigger overlap: mitigated by explicit handoff boundaries and near-miss checks.
