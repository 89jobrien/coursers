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
