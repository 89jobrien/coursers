# Design: Nushell POSIX Operator Correction in coursers

## Goal

Stop the Bash tool from failing on POSIX-only shell syntax under Nushell by
mechanically rewriting the operators that have provably-exact nu equivalents, and
surfacing an exact correction for those that cannot be safely translated.

## Approved Approach

**C — Narrow rewrite + self-correcting feedback** (approved in brainstorm), with a
**verified narrowing**: the rewrite set is reduced from four operators to one after
empirical testing showed `&&` and `||` have no semantically-equivalent single-pass
regex translation. See [Deviation from brainstorm](#deviation-from-brainstorm).

---

## Context Map

### Files to Modify

| File | Repo | Purpose | Changes Needed |
| --- | --- | --- | --- |
| `.config/crs/plugins.d/nu-shell.toml` | dotfiles | nu-syntax plugin rules | add 1 `rewrite` rule (`2>/dev/null`); add 1 `notify` rule (residual POSIX syntax) |
| `.config/coursers/course-correct-rules.json` | dotfiles | block rules for `coursers pre` | correct the `no-bash-use-nu` message text |
| `crates/e2e/tests/hook_chain.rs` | coursers | e2e hook pipeline tests | add regression tests (see Test Coverage) |

Live paths (`~/.config/crs/plugins.d/*.toml`) are **symlinks** into the dotfiles
repo. Edit the dotfiles repo; never the `~/.config` path.

### Dependencies (must be updated in the same change)

| File | Relationship |
| --- | --- |
| `crates/coursers/src/crs_commands.rs:810-820` | `coursers pre` runs *before* the TOML pipeline and `exit(2)`s, short-circuiting it. This ordering is why the block rule and the rewrite rules cannot overlap. |
| `crates/core/src/hook/pipeline.rs:247` | `let target = ctx.target...` — `pattern`/`unless` match the **command**, never the tool output. |
| `crates/core/src/hook/pipeline.rs:396` | `apply_rewrite` uses `Regex::replace` = **first match only**. |
| `crates/core/src/hook/pipeline.rs:451` | `expand_template` — notify templates expand only `${target}`, `${tool_name}`, `${exit_code}`. |

### Test Coverage

| Test | Status | Covers |
| --- | --- | --- |
| `crates/e2e/tests/hook_chain.rs` | extend | rewrite + notify + negative cases |
| `crates/e2e/tests/pipeline.rs` | existing | JSON block rules via `COURSERS_RULES` |
| `notify` action | **gap — no coverage exists** | new tests required |
| multi-occurrence rewrite | **gap** | documented known limitation, pinned by test |

No test currently exercises `~/.config/crs/plugins.d/`. There is **no env override**
for that path — `load_config()` reads `dirs::home_dir()` directly
(`crates/core/src/hook/pipeline.rs:161`). Verified empirically: setting `HOME` on the
child process **does** redirect plugin loading, so tests must point `HOME` at a
`TempDir` containing `.config/crs/plugins.d/*.toml`.

### Reference Patterns

| File | Pattern to Follow |
| --- | --- |
| `nu-shell.toml` (existing) | `[[hooks]]` with `pattern`/`unless`/`action = "rewrite"`/`replace` |
| `session-lifecycle.toml` (existing) | `action = "notify"` with `template`, and the header comment documenting the `target`-is-always-empty constraint |
| `crates/e2e/tests/hook_chain.rs:22` | `workspace_bin()` + `run_bin_with_env()` helpers for spawning the binary with a JSON payload on stdin |

### Risk

- [ ] Breaking API changes: **no** — no production Rust changes at all; config data + tests only.
- [ ] New external dependency: **no**.
- [ ] Feature flag: **no**.
- [ ] `notify` fires on every PostToolUse Bash whose command matches — a second
      matching rule would emit two `systemMessage`s. Keep exactly one.
- [ ] `~/.config/coursers/course-correct-rules.json` is a ruleset used by `coursers pre`;
      editing the `no-bash-use-nu` message changes guidance for every Bash call.

---

## Crate Ownership

- **Owner crate**: *none* — no new crate, and **no production Rust code changes**.
- **Config repo**: dotfiles (`/Users/joe/.notfiles`) — all rule definitions.
- **Test crate**: `coursers-e2e` (existing) — regression tests only.

This design deliberately produces zero new public API.

## Public API

None. No new traits, types, or functions. The contract (`HookRule` / `HookAction`)
already expresses every action used here; the two variants needed (`Rewrite`,
`Notify`) exist today.

**Contract location:** `coursers_core::hook::pipeline` — `HookAction` at
`crates/core/src/hook/pipeline.rs:44`, which carries the `Redact` variant.
`coursers_types::pipeline` also defines `HookAction` (`crates/types/src/pipeline.rs:57`)
but is an **unused duplicate** with no importers anywhere in the workspace and has
already diverged (no `Redact` arm). Do not cite or target the types copy; it is
tracked by `TODO(pipeline-types-deduplication)` and is slated for deletion.

## Data Flow

1. **Source** — Claude Code hook payload JSON on stdin (`tool_input.command`,
   `tool_response`).
2. **Transform** — `load_config()` merges `~/.config/crs/hooks.toml` and
   `~/.config/crs/plugins.d/*.toml`; `run_pipeline` filters each rule by
   `event` → `matcher` → `pattern` → `unless` → `when`, then applies its action.
   The `pattern` is evaluated against `ctx.target` (the command string only).
3. **Sink** — stdout JSON. `Rewrite` emits `updatedInput.command`;
   `Notify` emits `hookSpecificOutput.systemMessage`.

## Hexagonal Boundaries

No new ports. The existing boundary is:

- **Port** (contract): `HookRule` / `HookAction` in
  `coursers_core::hook::pipeline` (the deserialization contract for rule config).
  `coursers_types::pipeline` is an unused duplicate and is **not** the contract.
- **Adapter** (impl): `HookPipeline` / `run_pipeline` in `coursers_core::hook::pipeline`.
- **Adapter input**: the TOML rule files in the dotfiles repo.

External I/O is unchanged; the dotfiles TOML is configuration data, not a new
dependency edge.

---

## Rules

### R1 — Rewrite `2>/dev/null` → `| ignore`

In `nu-shell.toml`:

```toml
[[hooks]]
label = "nu/devnull-to-ignore"
event = "pre-tool-use"
matcher = "Bash"
pattern = "2>/dev/null"
unless = "^nu\\b"
action = "rewrite"
replace = "| ignore"
```

Exact semantic match: `| ignore` discards stdout precisely as `2>/dev/null` discards
stderr for a fire-and-forget command. Verified in Nushell 0.111.0.

`unless = "^nu\\b"` mirrors the existing `nu/wrap-simple-commands` rule, which exists
to prevent double-wrapping an already-wrapped command.

### R2 — Notify on residual POSIX syntax

In `nu-shell.toml`:

```toml
[[hooks]]
label = "nu/posix-residual-correction"
event = "post-tool-use"
matcher = "Bash"
pattern = "2>&1|2>\\s*/dev/null|&&|\\|\\|"
unless = "^nu\\b"
action = "notify"
template = "<exact nu correction text>"
```

This is the self-correcting half of Approach C. It covers every case a
single-pass rewrite cannot: the `2>&1` case, and the second-and-later occurrences
that `apply_rewrite` leaves behind.

The template must state, per construct:

- `2>&1` into a file → `cmd out+err> merged.txt`, then read the file. `out+err>`
  **requires** a file target; `cmd out+err> | str join` is a parse error.
- `2>&1` into a pipe, **external** command →
  `nu -c 'cmd | complete | get stdout stderr | str join "\n"'`.
  `get stdout` alone drops stderr; `complete` is **external-only** and throws
  *"Complete only works with external commands"* on internal nu commands.
- `2>&1` with an **internal** nu command → no merge exists; use `^cmd` to force an
  external invocation, then `complete`.
- `2>/dev/null` → `| ignore` (R1; listed only for occurrences R1 missed).
- `&&` / `||` → run the steps as separate tool calls, or use a `nu -c` block. See
  [Deviation](#deviation-from-brainstorm) for why these are not rewritten.

### R3 — Correct the `no-bash-use-nu` message

In `course-correct-rules.json`, rule `no-bash-use-nu` (line 216). Its current
message recommends:

```
cmd 2>&1 | tail -N → nu -c "do { cmd } | complete | get stdout | lines | last N | ..."
```

This is **wrong on two counts**, both reproduced against Nushell 0.111.0:

1. `do { cmd } | complete` throws *"Complete only works with external commands"*
   when `cmd` is an internal nu command, and with external commands leaked stdout
   to the terminal rather than capturing it.
2. `get stdout` does not merge stderr — `cmd | complete | get stdout` returned only
   stdout while stderr was discarded.

Replace with the R2 correction text. `&&` and `||` **stay in this rule's pattern** —
see Deviation.

---

## Deviation from brainstorm

The brainstorm approved four rewrite rules: `2>&1`, `2>/dev/null`, `&&`, `||`.
Testing showed two are unsound, and one is a no-op. This design implements one
rewrite plus one notify.

### `&&` and `||` are not rewritable — reverted to deny

Two independent blockers, both reproduced:

1. **Semantic loss.** `a && b` short-circuits; `a ; b` does not. Rewriting to `;`
   means `b` runs even when `a` fails — `cd /nope && ls` becomes `ls` in the wrong
   directory. Rewriting to `and` is not better: Nushell's `and` is a boolean
   operator, not a general command sequencer, and `cd` does not set the shell
   working directory in Nushell anyway.
2. **No single-pass translation.** `apply_rewrite` uses `Regex::replace` (first
   match only). `cd /nope && ls && echo done` rewrote to
   `cd /nope and ls && echo done` — a mixed, still-broken command.

Verified output:

```
input:    cd /nope && ls && echo done
rewrite:  cd /nope and ls && echo done
```

Therefore `&&` and `||` remain in the `no-bash-use-nu` block pattern, which
already returns corrected guidance. The brainstorm's "drop them from the block
rule" decision is **reverted**; the R2 notify rule covers the residual case.
Proper support is a follow-up — see
[Follow-up: and/or operator support](#follow-up-andor-operator-support).

### `2>&1` is not rewritable — confirm Approach C

`2>&1` has no exact nu equivalent for a piped command. `complete` is external-only
and does not merge unless both `stdout` and `stderr` are selected; internal
commands have no capture mechanism at all. Externality is not regex-detectable, so
a blind rewrite would silently produce broken commands. R2 handles it.

### Known limitation: multi-occurrence rewrite

`2>/dev/null` is rewritten **once** per command, because `apply_rewrite` replaces
the first match only. Verified:

```
input:    rm -rf /tmp/x 2>/dev/null; git checkout . 2>/dev/null; echo ok
rewrite:  rm -rf /tmp/x | ignore; git checkout . 2>/dev/null; echo ok
```

This is accepted deliberately: the rewrite is an exact, no-risk transformation, and
R2's notify rule catches the residual occurrence. Making it complete would require
a `replace_all` capability in `apply_rewrite` — a core change, out of scope.

---

## Verification

Targets written to files and piped in, never embedded in the outer Bash call
(per the `crs` hook-testing gotcha, so the outer call cannot trigger the rule under
test).

| # | Check | Method | Expected |
| --- | --- | --- | --- |
| 1 | `2>/dev/null` rewrite | `crs hook pre-tool-use`, `HOME`=TempDir | `\| ignore` substituted |
| 2 | Negative: `;` untouched | same, `cmd a; b` | unchanged |
| 3 | Negative: bare `\|` untouched | same, `cmd \| tail -3` | unchanged |
| 4 | Negative: `^nu` exempt | same, `nu -c '...'` | unchanged |
| 5 | Notify surfaces | `crs hook post-tool-use`, `2>&1` command | `systemMessage` present |
| 6 | Notify does not block | same | `permissionDecision: allow` |
| 7 | Config validity | `crs validate-hooks` | pass |
| 8 | Live behavior | `just install` in coursers, then re-run 1–6 | unchanged from repo |
| 9 | Gates | `cargo fmt --all`; `cargo clippy --workspace -- -D warnings`; `cargo nextest run --workspace` | pass |
| 10 | Coverage gate | `crs discover`, `crs heat` | see Measurement |

## Measurement Gate

Per the "implement and measure" decision:

- `crs discover` — unhandled Bash commands still falling through.
- `crs heat` — confirm R1 and R2 actually fire.

A new operator is added **only** at ≥3 distinct occurrences in discovery output.
Below that it stays a notify suggestion. Every rewrite carries the externality and
first-match risk documented above; speculative rules on cold paths are how rulesets rot.

## Out of Scope

- `&&` and `||` rewrites — deferred to the
  [follow-up](#follow-up-andor-operator-support) by decision, not by omission. They stay
  denied by `no-bash-use-nu` and reported by R2 in the meantime.
- The `;` false positive in `no-bash-use-nu` — `;` is valid in Nushell and the rule
  blocks it. Real bug, separate change, affects every Bash call.
- `1>&2`, `>>file`, heredoc (`<<`) — deferred to the measurement gate.
- Any change to `crates/core` (`apply_rewrite`, `run_pipeline`, config loading).
  The `replace_all` gap and the missing plugins.d env override are both noted, not
  fixed. The follow-up's direction 1 would touch `apply_rewrite`; that is the
  follow-up's job, not this design's.
- Reconciling or committing the existing dirty trees in either repo.
- Enabling `[flow]` or bootstrapping the flow branches — recorded as
  [preconditions](#preconditions--currently-unmet), not performed here.
- Making the Bash tool default to `out+err>` in generated commands — a prompt/memory
  change, not a coursers change.
- `nu --ide-check` / `crs nu-check` integration.

## Follow-up: and/or operator support

`&&` and `||` **are** in scope for coursers, but as a **follow-up change**, not in
this one. This design leaves them in the `no-bash-use-nu` block pattern; the
follow-up gives them real handling. The blockers are already characterised above
(semantic loss from `;`, and first-match-only `Regex::replace`), so the follow-up
knows exactly what it must solve.

The blocker is structural, not a matter of finding the right regex, so the
follow-up most likely needs a change in `crates/core` rather than more config.
Three candidate directions, in order of preference:

1. **`replace_all` in `apply_rewrite`** (`crates/core/src/hook/pipeline.rs:396`).
   Switch `re.replace` to `re.replace_all` behind an opt-in rule field, so a
   rewrite can normalise every occurrence in one pass. Fixes chained
   `a && b && c` and multi-`2>/dev/null` for *all* rules, not just the operators
   added here. This is the highest-leverage change and also closes the known
   limitation recorded above.
2. **A real POSIX→nu translation step.** A function that parses the command into
   segments and emits idiomatic nu, with `&&` → `and` and `||` → `or` preserving
   short-circuit semantics. Larger scope; needs a port/adapter split to stay
   testable, and must handle the externality distinction that makes `2>&1`
   unrewritable today.
3. **Keep deny + sharpen the message.** No core change; improve the
   `no-bash-use-nu` text with a worked `&&` example. Cheapest, but the round-trip
   remains.

Direction 1 is a strict prerequisite for any correct multi-operator rewrite, so it
should land first regardless of which direction the follow-up ultimately takes.
Note it is a production Rust change and therefore does carry the API-surface and
semver questions that this design explicitly avoids.

## Branching

Work lands on `develop` in each repo, driven by `taskit flow auto` where taskit is
available. `main` moves only by explicit merge.

| Repo | Current branch | Target |
| --- | --- | --- |
| `/Users/joe/dev/coursers` | `feat/opencode-harness-support` | merge to `main`, then `develop` |
| `/Users/joe/.notfiles` | `feat/secure-tailscale-key-sync` | merge to `main`, then `develop` |

### Preconditions — currently unmet

`taskit flow auto` cannot run in either repo as things stand. Four blockers, all
verified 2026-09-30:

1. **`[flow]` is commented out in `taskit.toml`** (`/Users/joe/dev/coursers/taskit.toml:90-93`).
   The whole section is disabled, so the main → develop → staging → release →
   main pipeline is not configured.
2. **The flow branches do not exist.** `develop`, `staging`, and `release` are all
   absent in both repos. `taskit flow` expects all four and fails with
   `wrong_branch` until they are bootstrapped from `main`.
3. **The dotfiles repo has no `taskit.toml` at all**, so `taskit flow auto` cannot
   run there regardless of the above. The dotfiles edits are the bulk of this
   design, so they need a different promotion path.
4. **Both working trees are dirty, and neither is mine.**

   - `coursers`: 14 modified tracked files across `crates/core`,
     `crates/coursers`, `crates/types`, `docs`, `fuzz`, plus `justfile`,
     `README.md`, and `agents/INDEX.md`. The two hook-pipeline files this design's
     context map cites (`crates/core/src/hook/pipeline.rs`,
     `crates/types/src/pipeline.rs`) are already modified by other work. Those
     changes are documentation-only `TODO` blocks, so they do not invalidate this
     design — but they will shift the line references above.
   - `dotfiles`: a large mixed staged/unstaged tree, including staged modifications
     to `coursers/.config/crs/plugins.d/nu-shell.toml` — a file R1 and R2 must
     edit — and untracked `failure-triage.toml` / `session-lifecycle.toml`.

`coursers` additionally has **27 active worktrees**, indicating parallel work in
flight. Merging to `main` while those are live risks sweeping in work that is not
mine.

Merging either branch to `main` right now would violate the workspace rule that
uncommitted work is not mine to absorb, and would carry unrelated in-flight changes
into `main`. **The dirty trees must be reconciled or stashed by their owners first.**
This design does not authorize that merge; it records the intended end state.

### `just install` caveat

`just install` in coursers is required before trusting any live `crs` behavior
against these changes — `cargo build` does not update `~/.cargo/bin/crs`.
