# CI Pipeline

Run the full pipeline:

```sh
cargo xtask ci
```

## Steps

| Step | Command | Gate |
|------|---------|------|
| Self-check | `taskit self-check` | Yes |
| Format | `taskit fmt --check` | No |
| Lint | `taskit lint` | No |
| Compile tests | `taskit compile-tests` | No |
| Test | `taskit test` | No |
| Deps | `taskit check-deps` | No |
| Drift | `taskit check-protocol-drift` | No |

## Known gaps

<!-- TODO(ci-health-baseline-stale): `.health-baseline.json` is dated
     2026-06-28 and records `tests_passed: 472`, `todo_fixme_count: 17`.
     The workspace now has 604 tests (`cargo nextest list --workspace`) and 39
     tracked TODO markers. No step in `taskit.toml` regenerates the baseline,
     and no step reads it, so the numbers drift silently. Either wire
     `taskit health check` into the pipeline or delete the file. -->

<!-- TODO(release-workflow-missing): `.github/workflows/` contains only
     `ci.yml`. There is no release or publish workflow, even though the
     workspace is publication-shaped — `license = "MIT OR Apache-2.0"` and
     `repository` are set, and `coursers-e2e` / `xtask` are deliberately
     `publish = false`. Commit 07fd054 ("prepare initial crates.io
     publication") did the manifest work but left publishing manual. -->

<!-- TODO(mdbook-docs-unbuilt): `docs/book.toml` and `docs/src/` (9 pages) are
     never built. No CI step runs `mdbook build`, so link rot and page drift go
     unnoticed — this file is a hand-maintained duplicate of the `ci.steps`
     table in `taskit.toml` with no check tying them together. -->

1. **No health-baseline gate** — `.health-baseline.json` is stale and unwatched.
2. **No release workflow** — publishing is manual.
3. **Docs are never built** — `mdbook build` is not in the pipeline.
4. **No fuzz compile** — `fuzz/` is a separate workspace; see
   `TODO(workspace-test-gaps)` in `crates/types/src/lib.rs`.
5. **No OpenCode Bun suite, no smoke test** — see
   `TODO(ci-surface-gaps)` in `crates/types/src/lib.rs`.
6. **`todo-sync` is already failing** — `taskit protocol todo-sync` reports 39
   unsynced markers on a clean tree; run `taskit todo-sync --update`.
7. **Single job** — one `ci` job runs everything, so a lint failure blocks
   signal from the test suite and there is no parallelism.
