# Design: Branch Reconciliation and Audit-Marker Landing

## Goal

Retire the diverged local `fix/redact-before-output-shaping` in favour of its remote
counterpart, landing the 12 dirty files as four conventional commits on the remote's base
with every factual claim in the architectural audit markers verified against that base.

## Approved Approach

**Approach A — retire first, land on the clean base.** Align the branch pointer to
`origin/fix/redact-before-output-shaping` (`fc4ff43`) before landing any work, so the audit
markers are verified against the history the remote actually has.

---

## Context Map

### Files to Modify

Twelve paths: ten tracked modifications, two untracked additions. **None contains a
functional code change** — verified by reading every diff.

| File | Change | Nature |
| --- | --- | --- |
| `fuzz/Cargo.lock` | +501 | Lockfile still names pre-rename `crs-core` / `crs-core-fuzz`; regeneration resolves 49 real packages |
| `crates/types/src/ports.rs` | +57 | `TODO(canonical-ports-migration)` marker. Hash-tracked protocol surface |
| `crates/core/src/lib.rs` | +26 | `TODO(adapters-crate-consolidation)` migration board |
| `crates/types/src/pipeline.rs` | +18 | `TODO(pipeline-types-deduplication)` marker |
| `crates/coursers/src/main.rs` | +10 | `TODO(completions-discoverability)` marker |
| `fuzz/fuzz_targets/fuzz_pipe_stages.rs` | +14 | `TODO(fuzz-crash-artifact-untriaged)` — **disproven, delete** |
| `justfile` | +12 | `TODO(justfile-recipe-drift)` marker |
| `README.md` | +89 | Prose describing the audit |
| `docs/src/reference/ci-pipeline.md` | +33 | Prose describing the audit |
| `agents/INDEX.md` | +10 | Prose describing the audit |
| `site/` (untracked, 21 files) | new | Static reference site — 7 HTML pages, 4 CSS, self-hosted IBM Plex `woff2`, mermaid diagrams |
| `.github/workflows/pages.yml` (untracked) | new | 30-line GitHub Pages deploy |

### Dependencies

| File / ref | Relationship |
| --- | --- |
| `design/nu-posix-operator-correction` | Holds all 8 borrowed commits. 4 pushed, 4 local-only |
| `taskit-protocol.lock` | Hash-tracked; `coursers-types-ports` entry will not match after editing `ports.rs` |
| `taskit-todo-sync.lock` | 44 entries over 23 files, all under `crates/`. Holds `#75`, `#100`, `#109` for 3 of the new markers |
| `.gitignore:8` | `/fuzz/artifacts` is gitignored — nothing under it is or should be tracked |
| `taskit.toml:52-85` | `[[ci.steps]]` — `self-check` is the only `gate = true` |
| `docs/src/` | Verified present; `docs/designs/` is the established design-doc location (8 existing docs) |

### Test Coverage

No test changes. All four work streams are comments, docs, a lockfile, and static assets.
Gates run per the workspace rule regardless: `cargo fmt --all`,
`cargo clippy --workspace -- -D warnings`, `cargo nextest run --workspace`.

### Reference Patterns

| Pattern | Source |
| --- | --- |
| Design-doc location and kebab naming | `docs/designs/2026-09-30-nu-posix-operator-correction-design.md` |
| TODO marker slug convention | existing `TODO(kebab-slug):` markers under `crates/` |
| Lock entry shape | `taskit-todo-sync.lock` — `{file, text, issue_number}` |

### Risk

- [x] **Public API change: no.** Zero functional code in the diff. Verified by reading all ten.
- [x] **Serialization change: no.** No `Serialize`/`Deserialize` shape changes.
- [x] **CLI output change: no.**
- [x] **Cross-crate boundary: no.** All edits are comments, prose, and static assets.
- [x] `git reset --hard` does **not** touch untracked files, so `site/` and `pages.yml` survive it.
- [x] Repo is in OPAVS phase **ACT** (`.ctx/opavs/phase`) — editing is permitted.

---

## Repository Ownership

- **Owner**: `coursers` — the branch and all twelve paths belong to it.
- **Not touched**: `design/nu-posix-operator-correction` retains the 8 borrowed commits. A reset
  cannot lose them: commits 1–4 are on `origin/design/nu-posix-operator-correction`, and all 8
  are on the local branch of that name.

---

## Deliverables

Four commits, in order. Staged with explicit paths — never `git add -A`.

### 1. `fix(fuzz): regenerate lockfile against post-rename crate names`

`fuzz/Cargo.lock` only. The lock names `crs-core` / `crs-core-fuzz`, which no longer exist.
Independently correct and independently valuable; carries no dependency on the other streams.

### 2. `docs(core): record architectural audit findings as tracked markers`

The five marker files plus `justfile`, **with the corrections below applied**. Includes the
`taskit-protocol.lock` hash refresh.

### 3. `docs: describe audit findings in README, agent index, and CI reference`

`README.md`, `agents/INDEX.md`, `docs/src/reference/ci-pipeline.md`. Prose only.

### 4. `feat(site): add static reference site and Pages deploy`

`site/` (21 files) and `.github/workflows/pages.yml`. Self-contained; no build-system coupling.

---

## Corrections

The audit markers are directionally correct — spot-checks confirmed
`capture.rs:24` is `SuggestionRecord`, and `HookAction::Redact` is absent from the `types`
copy exactly as claimed. But they carry **three factual errors and one disproven premise**.
All are corrected in commit 2; none is landed as written.

### 1. `fuzz-crash-artifact-untriaged` — premise disproven, delete the marker

The marker claims "A minimized crash artifact **is committed**". Two findings:

- `.gitignore:8` ignores `/fuzz/artifacts`, and `git ls-files fuzz/` returns no artifact
  entries. Nothing under it is tracked, so it was never committed.
- All four asserts **pass** for the artifact's input `""` (two quote bytes). Tracing
  `pipe_stages` (`crates/core/src/parse/pipeline.rs:27`): `Q::Double` opens at `i=0`, closes at
  `i=1`, then `segment[0..].trim()` yields the two-character string, which is non-empty and
  already trimmed, so it is pushed. `pipe_stage_commands` (`pipeline.rs:9`) then filters it out
  because `shell_words::split("\"\"")` yields one empty name, satisfying the
  `name.is_empty()` guard. `cmds.len() <= stages.len()` holds at `0 <= 1`.

The behaviour is correct. The artifact is stale and local-only by design. Delete the marker.

### 2. `history.rs:60` → `history.rs:63`

`crates/types/src/ports.rs` cites `history::{CommandRecord, DiscoverOpts}` at
`crates/core/src/analyze/history.rs:47,60`. Verified: `CommandRecord` is at 47; `DiscoverOpts` is
at **63**.

### 3. `opencode.rs:8` → `opencode.rs:27`, with corrected symbols

`crates/types/src/pipeline.rs` cites `crates/coursers/src/opencode.rs:8` as importing
`coursers_core::hook_pipeline::{HookContext, HookEvent}`. Verified at both bases: the import is
at line **27**, and reads
`use coursers_core::hook_pipeline::{HookContext as PipelineContext, HookEvent};`.

### 4. "only 8 types are re-exported" → 13 names across 6 sites

`crates/types/src/ports.rs` claims 8 types are re-exported from `coursers_types`, citing
`core/src/rules.rs:9` and `core/src/state.rs:8` as the pattern. Verified re-exports at
`fc4ff43`:

| Site | Names |
| --- | --- |
| `crates/core/src/rules.rs:9` | `FailureLearning, Rule, RulesConfig` |
| `crates/core/src/state.rs:8` | `FailureEntry, State` |
| `crates/core/src/hook/filters.rs:6` | `FilterMode, FilterRule, FiltersConfig` |
| `crates/core/src/hook/filter_logic.rs:6` | `FilterPayload, FilterResult` |
| `crates/core/src/hook/rewrite.rs:5` | `RewriteConfig, RewriteRule` |
| `crates/core/src/hook/tool_swap.rs:8` | `ToolSwapConfig` |

That is **13 type names across 6 sites**, not 8 across 3. The marker also omits
`crates/core/src/lib.rs:35` (`pub use coursers_types as types;`), a module alias covering every
remaining type in the crate — which materially understates how much of `core` already resolves
through `types`. The marker's Step 1 instruction ("re-point the 15 duplicated domain types")
is unaffected, but its supporting evidence must be restated or the plan it justifies is not
trustworthy.

### Base-independence

None of the four corrections is affected by the reset. `crates/core/src/analyze/history.rs`,
`crates/coursers/src/opencode.rs`, `crates/types/src/`, `crates/core/src/obfsck.rs`,
`crates/core/src/rtk.rs`, `crates/core/src/analyze/stats.rs`, `crates/core/src/rules.rs`, and
`crates/core/src/state.rs` are absent from the 8-commit delta, so their content is identical at
`5fb1d1c` and `fc4ff43`. The citations are wrong now and stay wrong after alignment.

---

## Reconciliation Sequence

1. **Anchor.** `git stash create` then `git stash store` — captures the ten tracked
   modifications into a recoverable object and **leaves file bytes untouched**. Zero-intrusion.
2. **Align.** `git fetch`, then `git stash push` the tracked modifications, then
   `git reset --hard origin/fix/redact-before-output-shaping`, then `git stash pop`. The 8
   borrowed commits leave the working tree; they remain on `design/nu-posix-operator-correction`.
   `site/` and `pages.yml` are untracked and survive the reset untouched.
3. **Restore and verify.** Confirm `git status` shows the 12 paths again. Apply the four
   corrections from the Corrections section.
4. **Refresh the protocol hash.** `taskit-protocol.lock`'s `coursers-types-ports` entry
   (`hash: 54615484…`) will not match the edited `ports.rs`.
5. **Gates.** `cargo fmt --all`, then re-stage (the PostToolUse hook formats but does not
   stage), then `cargo clippy --workspace -- -D warnings`, then `cargo nextest run --workspace`.
6. **Commit** the four commits above, reading `git diff --cached --stat` before each.
7. **Push** with `-u`, confirming the branch is not `main` first.

`justfile` is the only path modified by both the reset and the stash pop. Its two edits are in
disjoint regions — the dirty change adds a 12-line TODO block above `default:`, the reset removes
a `test-enrichment:` recipe near line 26 — so the three-way merge should apply cleanly. If it
conflicts, resolve by keeping both and re-verify; do not force.

---

## Gate Posture

Replaces the usual hexagonal-boundaries section: this design introduces no traits, adapters, or
ports.

- Only `self-check` is `gate = true` (`taskit.toml:52-55`). `fmt --check`, `lint`, `test`,
  `check-deps`, and `check-protocol-drift` are all `gate = false` and **report** without failing.
- **There is no `todo-sync` CI step at all.**
- Consequence: the protocol-drift report and the unsynced markers cannot fail CI. Correctness
  here is a matter of doing it deliberately, not of being forced to. Refresh the protocol hash
  in commit 2 so the committed lock describes the committed file.
- Marker-to-issue linkage after the corrections: `#75`, `#100`, `#109` already exist for
  `adapters-crate-consolidation`, `completions-discoverability`, and
  `pipeline-types-deduplication`. Two remain unsynced —
  `canonical-ports-migration` (`crates/types/src/ports.rs`) and `justfile-recipe-drift`
  (`justfile`) — because the lock covers only `crates/**/*.rs`. Run
  `taskit protocol todo-sync` to give them issues, consistent with the repo's
  one-issue-per-marker convention.

---

## Out of Scope

- The 2 redaction commits — content-identical to remote; nothing to salvage.
- The 8 borrowed commits' content — homed on `design/nu-posix-operator-correction`, untouched.
- Acting on any marker. They are landed as markers, not executed.
- `canonical-ports-migration` and `adapters-crate-consolidation` themselves — multi-step
  architectural work, tracked by their markers and issues.
- Adding `fuzz/` to CI. It needs nightly and is not built today.
- `hook-config-trust` — removed from `pipeline.rs` and its lock entry dropped by the 8 commits.
  The reset restores both marker and entry together, so the pair stays consistent.
- The 25 existing worktrees in this repo. Separate cleanup.

---

## Risk

- [ ] **Breaking API changes: no.** No functional code.
- [ ] **New external dependency: no.**
- [ ] **Feature flag required: no.**
- [ ] `git reset --hard` discards the ten tracked modifications if the stash pop fails —
  mitigated by step 1's `git stash create` anchor, which is retained until the four commits land.
- [ ] SSH signing may fail if 1Password is locked. Report and ask the user to unlock; do not
  change git config.
- [ ] If `taskit protocol todo-sync` opens issues for the two unsynced markers, that is a
  **remote mutation** — confirm with the user before running it.