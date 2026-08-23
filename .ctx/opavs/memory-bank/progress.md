# Progress

_Milestones as they land. Append, don't rewrite history._

## 2026-08-23 — opavs scaffolded

- Ran `opavs init`; existing `AGENTS.md` (pre-rename, referenced stale
  `crs-core`/`crs` crate names) was moved aside and its build/test/
  architecture content merged back into the freshly scaffolded
  `AGENTS.md`, corrected for the current five-crate workspace and the
  `coursers`/`crs` two-bin-one-package layout.
- Allow-listed `.ctx/opavs/tasks.yaml` and `.ctx/opavs/memory-bank/`
  through the `.ctx/*` gitignore blanket rule; `.ctx/opavs/phase` stays
  untracked.
- Fixed `.githooks/pre-push`, which called the removed flat
  `cargo xtask pre-push` subcommand — taskit/xtask had moved it under
  `xtask check pre-push`, silently breaking every push until this fix.
- Shipped as `26859b3` (opavs scaffold) and `a52f441` (pre-push hook fix).
