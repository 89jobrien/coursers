---
name: coursers-maintain
description: Develop and maintain the coursers repository. Use when working in /Users/joe/dev/coursers on Rust code, hook behavior, crs/coursers validation, AIL artifacts, doob/HANDOFF maintenance, or repo-specific docs and config.
---

# Coursers Maintenance

## Scope

Use this skill for project-local work in `coursers`. It covers the workspace crates,
hook config, AIL artifacts under `.ctx/ail/`, and repo maintenance tasks.

## Start Here

1. Read `README.md` and `AGENTS.md`.
2. Check `git status --short` and treat existing changes as user-owned unless the task
   explicitly includes them.
3. Inspect the owning crate before editing:
   - `crates/core` for shared logic
   - `crates/coursers` for hook entrypoints
   - `crates/crs` for filter, rewrite, discover, and validate
   - `crates/xtask` for workspace automation

## Working Rules

- Prefer `rg` and `rg --files` for inspection.
- Use `nu` for repo automation and shell scripts unless the repo already uses another tool.
- Keep edits narrow and avoid unrelated refactors.
- Use `apply_patch` for manual edits.
- Do not rewrite, stage, or revert unrelated dirty files.
- For `doob` or `HANDOFF` status changes, use the `doob` CLI instead of manual file edits.
- When a task crosses crates, confirm ownership before widening scope.
- Trust the implementation over stale comments or docs when they disagree.

## Validation

Run the lightest gates that match the change:

- Rust source changes: `cargo check`, then `cargo clippy --workspace -- -D warnings`, then
  `cargo test` or `cargo nextest run --workspace` when behavior changed.
- Hook or rule changes: `crs validate`.
- AIL artifact changes: rerun the relevant `ail run` phase and fix generated
  `evals.yaml`, `diagnosis.json`, or handoff output if the baseline is invalid.
- Skill changes: run the skill validator on the skill folder before considering it done.

## Maintenance Notes

- `crs-core` owns domain logic; keep `coursers` thin.
- Treat `.ctx` handoff files as generated outputs unless the task is explicitly about them.
- If the work touches shared behavior, validate the broad path rather than only a narrow unit test.
