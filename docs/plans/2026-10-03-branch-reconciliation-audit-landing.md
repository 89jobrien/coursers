# Plan: Branch Reconciliation and Audit-Marker Landing

## Goal

Retire the diverged local `fix/redact-before-output-shaping` in favour of
`origin/fix/redact-before-output-shaping` (`fc4ff43`), landing the 12 dirty files as four
conventional commits on the remote's base with every factual claim in the architectural audit
markers corrected before it is committed.

## Architecture

- **Crates affected**: none functionally. `coursers-types`, `coursers-core`, `coursers`, and the
  `fuzz` workspace are edited **comments-only**. The design established there is zero functional
  code across all twelve dirty paths.
- **New traits/types**: none.
- **Data flow**: 12 dirty paths → non-intrusive stash anchor → branch aligned to `fc4ff43` →
  4 corrections applied → protocol hash refreshed → four commits, each with explicit paths.

### Corrections applied (from the approved design)

| # | Target | Change |
| --- | --- | --- |
| 1 | `fuzz/fuzz_targets/fuzz_pipe_stages.rs` | **Delete** `TODO(fuzz-crash-artifact-untriaged)` — premise disproven |
| 2 | `crates/types/src/ports.rs` | `history.rs:60` → `history.rs:63` |
| 3 | `crates/types/src/pipeline.rs` | `opencode.rs:8` → `opencode.rs:27`, symbols → `{HookContext as PipelineContext, HookEvent}` |
| 4 | `crates/types/src/ports.rs` | "only 8 types re-exported" → 13 names across 6 sites + `core/src/lib.rs:35` |

### Commit topology

1. `fix(fuzz): regenerate lockfile against post-rename crate names`
2. `docs(core): record architectural audit findings as tracked markers`
3. `docs: describe audit findings and land reconciliation design`
4. `feat(site): add static reference site and Pages deploy`

## Tech Stack

- Rust edition 2024, resolver 3, MSRV 1.89 (per `AGENTS.md`).
- Toolchain is `taskit` via `cargo xtask`. **Note**: `AGENTS.md` documents
  `cargo xtask check-protocol-drift`, but the installed binary exposes
  `cargo xtask protocol drift` and `cargo xtask check <fmt|lint|test|ci>`. This plan uses the
  commands the binary actually accepts.
- No new dependencies. `fuzz/Cargo.lock` regeneration adds no manifest entries — the manifest is
  untouched.

### TDD adaptation (deviation, deliberate)

The plan format requires a failing test → verify FAIL → implement → verify GREEN → commit cycle
per task. **This work contains no functional code**, so a Rust unit test is not a meaningful
red/green signal here: a test asserting that a comment reads `:63` rather than `:60` would be
absurd and would contradict the design's own conclusion.

The faithful analogue, used throughout, is a **shell verification assertion** — a command that
exits non-zero before the change and zero after. Each task states its assertion explicitly. Every
task that changes a tracked file ends in a commit; the two git-mechanics tasks (1 and 2) mutate no
tracked file and therefore produce no commit, rather than an empty one.

## Preconditions

- OPAVS phase must be `ACT`. Verified: `.ctx/opavs/phase` contains `ACT`.
- `main` must not be checked out at any commit step.
- SSH signing via 1Password must be unlocked. On failure, report and stop — do not change git config.

---

## Tasks

### Task 1: Anchor the working tree with a non-intrusive snapshot

**Crate**: n/a — repository-level
**File(s)**: none modified
**Run**: `git stash list`

No commit for this task: it mutates no tracked file. Do not create an empty commit.

1. Verify FAIL — assert the tree still carries all 12 dirty paths:

   ```sh
   test "$(git status --porcelain | wc -l)" -eq 12
   ```

   Expected: exit 0 now, and must still exit 0 after this task — this task is proven safe by the
   count being *unchanged*, not by it going 0→non-zero. If it is not 12, stop and reconcile with
   the user before proceeding.

2. Capture a recovery anchor without touching file bytes:

   ```sh
   SNAP=$(git stash create "coursers audit WIP snapshot pre-reconciliation")
   git stash store -m "pre-reconciliation anchor" "$SNAP"
   echo "$SNAP"
   ```

   `git stash create` builds a commit object from the tracked modifications but leaves the working
   tree untouched. `git stash store` records it in the stash list. Record `$SNAP` — it is the
   fallback if Task 2's `stash pop` fails.

3. Verify GREEN — assert the tree is byte-identical and the anchor exists:

   ```sh
   test "$(git status --porcelain | wc -l)" -eq 12 && git cat-file -t "$SNAP" | grep -q commit
   ```

   Expected: exit 0. If the dirty count changed, the anchor was not non-intrusive — stop and
   report.

### Task 2: Align the branch pointer to the remote base

**Crate**: n/a — repository-level
**File(s)**: none modified
**Run**: `git rev-parse HEAD`

No commit for this task: branch-pointer movement is not a file change.

1. Verify FAIL — assert the branch is not yet aligned:

   ```sh
   test "$(git rev-parse HEAD)" != "$(git rev-parse origin/fix/redact-before-output-shaping)"
   ```

   Expected: exit 0 (they differ — `5fb1d1c` vs `fc4ff43`). If it exits non-zero the branch is
   already aligned; skip to Task 3.

2. Confirm the borrowed commits are safe before destroying anything:

   ```sh
   for c in 6f017fd 640a781 989872f 3b0b5ec f6297ef ecb179a b55c7b5 0e6d962; do
     printf '%s ' "$c"
     git branch -a --contains "$c" --format='%(refname:short)' | paste -sd, -
   done
   ```

   Expected: every SHA lists at least `design/nu-posix-operator-correction`. Commits 1–4 must also
   list `origin/design/nu-posix-operator-correction`. **If any SHA is listed only by
   `fix/redact-before-output-shaping`, stop — the reset would orphan it.**

3. Align:

   ```sh
   git fetch
   git stash push -m "coursers audit WIP pre-align"
   git reset --hard origin/fix/redact-before-output-shaping
   git stash pop
   ```

   `justfile` is the only path touched by both the reset and the pop. Its two edits are in disjoint
   regions — the dirty change adds a 12-line TODO block above `default:`, the reset removes a
   `test-enrichment:` recipe near line 26. If the pop conflicts, keep both sides, re-verify, and
   do not force.

4. Verify GREEN — assert alignment and restoration:

   ```sh
   test "$(git rev-parse HEAD)" = "$(git rev-parse origin/fix/redact-before-output-shaping)" \
     && test "$(git status --porcelain | wc -l)" -eq 12
   ```

   Expected: exit 0. Confirm `site/` and `.github/workflows/pages.yml` are still present — they are
   untracked and a hard reset does not remove untracked files.

### Task 3: Regenerate the stale fuzz lockfile

**Crate**: `fuzz`
**File(s)**: `fuzz/Cargo.lock`
**Run**: `grep -c 'crs-core' fuzz/Cargo.lock`

1. Verify FAIL — assert the lock still names pre-rename crates:

   ```sh
   grep -q 'name = "crs-core"' fuzz/Cargo.lock
   ```

   Expected: exit 0 today (the defect is present). After this task it must exit non-zero.

2. Regenerate:

   ```sh
   cargo +nightly fuzz build fuzz_pipe_stages
   ```

   This resolves the real dependency graph and rewrites `fuzz/Cargo.lock`. The manifest at
   `fuzz/Cargo.toml` is **not** modified. Expected additions: 49 packages including `miette`,
   `redb`, and `backtrace`; expected removals: `crs-core`, `crs-core-fuzz`.

   If `cargo +nightly fuzz` is unavailable, fall back to
   `cargo metadata --manifest-path fuzz/Cargo.toml` and report the deviation rather than
   hand-editing the lock.

3. Verify GREEN:

   ```sh
   ! grep -q 'name = "crs-core"' fuzz/Cargo.lock \
     && grep -q 'name = "coursers-core"' fuzz/Cargo.lock \
     && git diff --name-only -- fuzz/ | grep -qx 'fuzz/Cargo.lock'
   ```

   Expected: exit 0, and `fuzz/Cargo.toml` absent from the diff.

4. Run: `git branch --show-current`
   Verify output is `fix/redact-before-output-shaping`. **Stop immediately if it is `main`.**

   ```sh
   git add fuzz/Cargo.lock
   git diff --cached --stat
   git commit -m "fix(fuzz): regenerate lockfile against post-rename crate names"
   ```

   Read `git diff --cached --stat` before committing. Stage only `fuzz/Cargo.lock` — never
   `git add -A`.

### Task 4: Correct the audit markers and refresh the protocol hash

**Crate**: `coursers-types`, `coursers-core`, `coursers`, `fuzz`
**File(s)**: `fuzz/fuzz_targets/fuzz_pipe_stages.rs`, `crates/types/src/ports.rs`,
`crates/types/src/pipeline.rs`, `crates/core/src/lib.rs`, `crates/coursers/src/main.rs`,
`justfile`, `taskit-protocol.lock`
**Run**: `grep -c 'fuzz-crash-artifact-untriaged' fuzz/fuzz_targets/fuzz_pipe_stages.rs`

1. Verify FAIL — assert the disproven marker is still present:

   ```sh
   grep -q 'fuzz-crash-artifact-untriaged' fuzz/fuzz_targets/fuzz_pipe_stages.rs
   ```

   Expected: exit 0 today. After deletion it must exit non-zero.

2. Correction 1 — delete the disproven marker. Remove the entire 14-line
   `TODO(fuzz-crash-artifact-untriaged)` block from
   `fuzz/fuzz_targets/fuzz_pipe_stages.rs` (lines 3–16), leaving the module doc comment on line 1
   directly above `#![no_main]`.

   Basis: `.gitignore:8` ignores `/fuzz/artifacts` and `git ls-files fuzz/` returns no artifact
   entries, so it was never committed. All four asserts pass for the artifact's input `""` —
   `pipe_stages` (`crates/core/src/parse/pipeline.rs:27`) opens `Q::Double` at `i=0`, closes at
   `i=1`, and pushes the two-character trimmed tail; `pipe_stage_commands` (`pipeline.rs:9`) drops
   it via the `name.is_empty()` guard; `cmds.len() <= stages.len()` holds at `0 <= 1`.

3. Correction 2 — in `crates/types/src/ports.rs`, change the citation
   `history::{CommandRecord, DiscoverOpts}` → `crates/core/src/analyze/history.rs:47,60` to
   `...:47,63`. Verified: `CommandRecord` is at 47, `DiscoverOpts` at 63.

4. Correction 3 — in `crates/types/src/pipeline.rs`, replace the citation
   `crates/coursers/src/opencode.rs:8`, which imports
   `coursers_core::hook_pipeline::{HookContext, HookEvent}` with
   `crates/coursers/src/opencode.rs:27`, which imports
   `coursers_core::hook_pipeline::{HookContext as PipelineContext, HookEvent}`.

5. Correction 4 — in `crates/types/src/ports.rs`, replace the claim that "only 8 types are
   re-exported from this crate" with the verified 13 names across 6 sites:

   | Site | Names |
   | --- | --- |
   | `crates/core/src/rules.rs:9` | `FailureLearning, Rule, RulesConfig` |
   | `crates/core/src/state.rs:8` | `FailureEntry, State` |
   | `crates/core/src/hook/filters.rs:6` | `FilterMode, FilterRule, FiltersConfig` |
   | `crates/core/src/hook/filter_logic.rs:6` | `FilterPayload, FilterResult` |
   | `crates/core/src/hook/rewrite.rs:5` | `RewriteConfig, RewriteRule` |
   | `crates/core/src/hook/tool_swap.rs:8` | `ToolSwapConfig` |

   Also note the omitted module alias `crates/core/src/lib.rs:35`
   (`pub use coursers_types as types;`), which covers every remaining type in the crate. Without
   this the marker's Step 1 instruction is not trustworthy.

6. Refresh the protocol hash. `crates/types/src/ports.rs` is hash-tracked as
   `coursers-types-ports` in `taskit-protocol.lock` (current hash
   `54615484fe5da937c518d834c1b0f56329387c8e7eecc2b2fa91d1dcfb8de6d9`), so editing it produces
   drift:

   ```sh
   cargo xtask protocol drift
   cargo xtask protocol drift --update
   ```

7. Verify GREEN:

   ```sh
   ! grep -q 'fuzz-crash-artifact-untriaged' fuzz/fuzz_targets/fuzz_pipe_stages.rs \
     && ! grep -q 'history.rs:47,60' crates/types/src/ports.rs \
     && ! grep -q 'opencode.rs:8' crates/types/src/pipeline.rs \
     && ! grep -q 'only 8 types are re-exported' crates/types/src/ports.rs \
     && cargo xtask protocol drift | grep -q 'no drift'
   ```

   Expected: exit 0.

8. Run the workspace gates, then re-stage — the PostToolUse hook formats but does not stage:

   ```sh
   cargo fmt --all
   cargo clippy --workspace -- -D warnings
   cargo nextest run --workspace
   git add fuzz/fuzz_targets/fuzz_pipe_stages.rs crates/types/src/ports.rs \
     crates/types/src/pipeline.rs crates/core/src/lib.rs crates/coursers/src/main.rs \
     justfile taskit-protocol.lock
   ```

   Expected: zero clippy warnings, all tests green.

9. Run: `git branch --show-current`
   Verify output is `fix/redact-before-output-shaping`. **Stop immediately if it is `main`.**

   ```sh
   git diff --cached --stat
   git commit -m "docs(core): record architectural audit findings as tracked markers"
   ```

### Task 5: Sync the two unlinked markers — CONDITIONAL, requires user approval

**Crate**: n/a — repository-level
**File(s)**: `taskit-todo-sync.lock`
**Run**: `cargo xtask protocol todo-sync`

**Gate: do not run without explicit user go-ahead.** This opens real GitHub issues — a remote
mutation. If declined, skip this task entirely; no CI step gates it either way
(`taskit.toml:52-85` shows `self-check` as the only `gate = true`, and there is no `todo-sync`
step at all).

1. Verify FAIL — assert the two markers are unlinked:

   ```sh
   ! grep -q 'canonical-ports-migration' taskit-todo-sync.lock \
     && ! grep -q 'justfile-recipe-drift' taskit-todo-sync.lock
   ```

   Expected: exit 0 today. The lock holds 44 entries over 23 files, all under `crates/`, which is
   why these two are absent. Three sibling markers are already linked: `#75`
   (`adapters-crate-consolidation`), `#100` (`completions-discoverability`), `#109`
   (`pipeline-types-deduplication`).

2. Sync:

   ```sh
   cargo xtask protocol todo-sync
   ```

   Report the issue numbers it opens. Do not run `protocol todo-dedupe` — out of scope.

3. Verify GREEN:

   ```sh
   grep -q 'canonical-ports-migration' taskit-todo-sync.lock \
     && grep -q 'justfile-recipe-drift' taskit-todo-sync.lock
   ```

   Expected: exit 0.

4. Run: `git branch --show-current`
   Verify output is `fix/redact-before-output-shaping`. **Stop immediately if it is `main`.**

   ```sh
   git add taskit-todo-sync.lock
   git diff --cached --stat
   git commit -m "chore(taskit): link audit markers to tracking issues"
   ```

   Skip steps 3–4 if the user declined; the working-tree change is confined to
   `taskit-todo-sync.lock`.

### Task 6: Land the prose and the reconciliation documents

**Crate**: n/a — repository-level
**File(s)**: `README.md`, `agents/INDEX.md`, `docs/src/reference/ci-pipeline.md`,
`docs/designs/2026-10-03-branch-reconciliation-audit-landing-design.md`,
`docs/plans/2026-10-03-branch-reconciliation-audit-landing.md`
**Run**: `git diff --cached --stat`

1. Verify FAIL — assert the prose is uncommitted:

   ```sh
   git diff --name-only | grep -qx 'agents/INDEX.md'
   ```

   Expected: exit 0 today.

2. Confirm the four corrections from Task 4 are reflected where the prose describes them. Search
   `README.md`, `agents/INDEX.md`, and `docs/src/reference/ci-pipeline.md` for the stale figures
   `8 types`, `history.rs:60`, `opencode.rs:8`, and
   `fuzz-crash-artifact-untriaged`. Correct any that appear, so the prose does not contradict the
   markers:

   ```sh
   grep -nE '8 types|history\.rs:60|opencode\.rs:8|fuzz-crash-artifact-untriaged' \
     README.md agents/INDEX.md docs/src/reference/ci-pipeline.md
   ```

   Expected after correction: no output.

3. Verify GREEN:

   ```sh
   ! grep -qE '8 types|history\.rs:60|opencode\.rs:8|fuzz-crash-artifact-untriaged' \
     README.md agents/INDEX.md docs/src/reference/ci-pipeline.md
   ```

4. Run: `git branch --show-current`
   Verify output is `fix/redact-before-output-shaping`. **Stop immediately if it is `main`.**

   ```sh
   git add README.md agents/INDEX.md docs/src/reference/ci-pipeline.md \
     docs/designs/2026-10-03-branch-reconciliation-audit-landing-design.md \
     docs/plans/2026-10-03-branch-reconciliation-audit-landing.md
   git diff --cached --stat
   git commit -m "docs: describe audit findings and land reconciliation design"
   ```

   Stage the two new documents explicitly. `docs/plans/` is tracked in this repo (9 existing
   plans), so the plan file belongs in this commit.

### Task 7: Land the static reference site

**Crate**: n/a — static assets
**File(s)**: `site/index.html`, `site/architecture.html`, `site/cli.html`, `site/commands.html`,
`site/features.html`, `site/crates.html`, `site/guide.html`, `site/style.css`, `site/tokens.css`,
`site/signature.css`, `site/fonts/`, `site/diagrams/`, `.github/workflows/pages.yml`
**Run**: `git status --porcelain site/`

1. Verify FAIL — assert the site is untracked:

   ```sh
   git ls-files site/ | wc -l
   ```

   Expected: `0` today. `site/` is untracked but **not** gitignored — confirmed via
   `git status --porcelain --ignored site/` returning `?? site/`, so it is committable.

2. Verify the Pages workflow parses before committing:

   ```sh
   python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/pages.yml'))"
   ```

   Expected: exit 0.

3. Verify GREEN — assert nothing under `site/` is gitignored, which would silently drop assets:

   ```sh
   test -z "$(git check-ignore site/index.html site/style.css site/fonts/ibm-plex-mono-latin-400-normal.woff2)"
   ```

   Expected: exit 0. If this fails, a font or stylesheet is ignored and would not ship.

4. Run: `git branch --show-current`
   Verify output is `fix/redact-before-output-shaping`. **Stop immediately if it is `main`.**

   ```sh
   git add site .github/workflows/pages.yml
   git diff --cached --stat
   git commit -m "feat(site): add static reference site and Pages deploy"
   ```

   Confirm `git diff --cached --stat` lists all 21 `site/` files plus the workflow before
   committing — a partial `site/` is the main risk here.

### Task 8: Run the full gate suite and push

**Crate**: `coursers` workspace
**File(s)**: none modified
**Run**: `cargo xtask check ci`

No commit for this task: it verifies and publishes.

1. Confirm the tree is clean and on the right branch:

   ```sh
   test -z "$(git status --porcelain)" && test "$(git branch --show-current)" = "fix/redact-before-output-shaping"
   ```

   Expected: exit 0.

2. Run the user's mandated gates in order:

   ```sh
   cargo fmt --all
   cargo clippy --workspace -- -D warnings
   cargo nextest run --workspace
   ```

   Expected: zero clippy warnings, all tests green. If `cargo fmt --all` reports changes, re-stage
   the affected paths before pushing — it does not stage on its own.

3. Run the repo's own umbrella gate, which is what CI executes (`.github/workflows/ci.yml:45`):

   ```sh
   cargo xtask check ci --fail-fast
   ```

   Expected: summary table with no failing step. Note `check-protocol-drift` reports but does not
   gate (`gate = false`).

4. Drop the anchor only after all four commits are pushed:

   ```sh
   git stash list
   ```

   Confirm the `pre-reconciliation anchor` entry is present, then remove it:

   ```sh
   git stash drop stash@{0}
   ```

   Verify the anchor SHA matches the `$SNAP` recorded in Task 1 before dropping. **Never drop it if
   that SHA is not found** — it is the only fallback for Task 2's discarded working tree.

5. Push:

   ```sh
   git branch --show-current
   git push -u origin fix/redact-before-output-shaping
   ```

   Verify output is `fix/redact-before-output-shaping`. **Stop immediately if it is `main`.** If
   push is rejected as non-fast-forward, do **not** force — fetch, inspect, and report.

---

## Out of Scope

- The 2 redaction commits — content-identical to remote; nothing to salvage.
- The 8 borrowed commits' content — homed on `design/nu-posix-operator-correction`, untouched.
- Acting on any audit marker. They are landed as markers, not executed.
- `canonical-ports-migration` and `adapters-crate-consolidation` — multi-step architectural work,
  tracked by their markers and issues.
- `cargo xtask protocol todo-dedupe`.
- Adding `fuzz/` to CI. It needs nightly and is not built today.
- `hook-config-trust` — removed from `pipeline.rs` and its lock entry dropped by the 8 commits.
  Task 2's reset restores marker and entry together, so the pair stays consistent.
- The 25 existing worktrees in this repository.
- Correcting the `AGENTS.md` drift where `check-protocol-drift` is documented but the binary
  exposes `protocol drift`. Worth a separate doob todo; not this plan.

## Risk

- [ ] **Breaking API changes: no.** No functional code in any of the twelve paths; verified by
      reading all ten tracked diffs.
- [ ] **New external dependency: no.** `fuzz/Cargo.lock` regeneration adds no manifest entries.
- [ ] **Feature flag required: no.**
- [ ] Task 2's `git reset --hard` discards the ten tracked modifications if `stash pop` fails.
      Mitigated by Task 1's `git stash create` anchor, retained until Task 8 step 4.
- [ ] Task 4 edits `crates/types/src/ports.rs`, a hash-tracked protocol surface, so
      `taskit-protocol.lock` must be refreshed in the same commit or drift is reported.
- [ ] Task 5 mutates GitHub remotely. Gated on explicit approval; skipping it breaks nothing.
- [ ] SSH signing may fail if 1Password is locked. Report and ask the user to unlock; do not
      change git config.
- [ ] `justfile` is the only path touched by both Task 2's reset and its pop. Disjoint regions make
      a clean three-way merge likely, but resolve rather than force if it conflicts.
- [ ] The 3-attempt rule applies: if any assertion fails three times, stop and report the root
      cause instead of continuing to patch.