# TODO(justfile-recipe-drift): This file has diverged from the pipeline CI
# actually runs and two recipes are broken.
#
# 1. `ci` below runs `cargo rail`, but CI runs `cargo xtask ci`
#    (.github/workflows/ci.yml) and `docs/src/reference/ci-pipeline.md`
#    documents `cargo xtask ci`. Confirm whether `cargo rail` is still intended
#    or is a leftover from a previous runner.
# 2. `e2e` below runs `cargo nextest run -p e2e`, but the package name is
#    `coursers-e2e` (crates/e2e/Cargo.toml). This recipe cannot succeed.
#
# Add recipes for the surfaces CI does not cover: `fuzz`, `smoke`, and the
# OpenCode Bun suite (see TODO(ci-surface-gaps) in crates/types/src/lib.rs).
# Default: run smart change-detection tests
default: ci

# CI: smart change-detection test selection (for PRs and commits)
ci:
    cargo rail run --merge-base --profile ci

# Run all tests across the workspace
test:
    cargo nextest run --workspace

# Run e2e tests (builds workspace first to ensure binaries are fresh)
e2e:
    cargo build --workspace
    cargo nextest run -p e2e

# Run all tests including e2e
all: test e2e

# Check + clippy, no tests
check:
    cargo check --workspace
    cargo clippy --workspace -- -D warnings

# Smoke test (requires installed or release-built binaries)
smoke:
    nu scripts/smoke.nu

# Test handoff enrichment from saved RTK JSON (RTK not required)
test-enrichment:
    nu scripts/test-enrich-handoff.nu

# Install coursers/crs to ~/.cargo/bin (cargo build/test do NOT do this — see CLAUDE.md)
install:
    cargo install --path crates/coursers
