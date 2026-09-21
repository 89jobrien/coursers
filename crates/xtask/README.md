# xtask

The `xtask` package is a thin workspace-local launcher for the external `taskit` CLI. It lets
contributors run repository gates through Cargo without duplicating task-runner logic in Coursers.

This package is tooling only (`publish = false`). Its library target is intentionally empty so
library-oriented compile and test gates still have a target to build.

## How it works

`cargo xtask ...` is provided by Cargo's standard xtask alias/configuration and launches
`crates/xtask/src/main.rs`. The shim:

1. forwards every argument to `taskit` unchanged;
2. exits with the delegated process's exit code;
3. runs `cargo install taskit` if `taskit` is not found;
4. retries the original command after installation.

The automatic installation is a real network and `$CARGO_HOME/bin` mutation. Provision `taskit`
explicitly in controlled or offline environments rather than relying on the fallback.

## Workspace configuration

The repository root `taskit.toml` is authoritative. It declares all five packages, dependency
propagation from `coursers-types` and `coursers-core`, CI steps, and tracked protocol surfaces:

```text
crates/types/src/ports.rs
crates/core/src/rtk.rs
crates/core/src/store.rs
crates/core/src/obfsck.rs
crates/core/src/loader.rs
```

The configured CI pipeline runs self-check, formatting, linting, test compilation, tests, unused
dependency checks, and protocol drift checks. Change gate selection in `taskit.toml`, not in this
shim.

## Common commands

The shim delegates to the installed Taskit version, so `cargo xtask --help` is the source of truth.
Current grouped commands include:

```sh
cargo xtask check fmt --check
cargo xtask check quick
cargo xtask check ci --fail-fast
cargo xtask test run
cargo xtask test run --crate-name coursers-core
cargo xtask protocol drift
cargo xtask protocol audit
cargo xtask dev setup
```

Useful behavior:

| Command | Purpose |
| --- | --- |
| `check quick` | Affected, offline fmt-check, lint, compile-tests, and tests |
| `check ci` | Full configured CI table; `--fail-fast` stops at the first failed step |
| `test run` | Run tests through nextest; supports `--affected` and `--crate-name` |
| `protocol drift` | Compare tracked contract surfaces; `--update` intentionally changes locks |
| `protocol audit` | Run dependency advisory, license, and ban checks |
| `dev setup` | Install development tools required by configured gates |

Do not use `protocol drift --update` merely to silence an unexplained change. Review the public
contract first and update the lock only when the change is intentional.

## Compatibility and safety contracts

- Forward arguments verbatim; the shim must not reinterpret Taskit flags.
- Preserve the delegated exit status so hooks and CI can trust gate failures.
- Keep task definitions in root configuration and Taskit, not copied into Rust code.
- Avoid output on stdout that would interfere with Taskit's structured output modes.
- The fallback installer may fail because of network, registry, permissions, or toolchain issues;
  those failures exit nonzero.
- The package contains no Coursers domain API and should not become a dependency of runtime crates.

## Development and validation

Changes to the shim are rare. Validate delegation and the package itself from the workspace root:

```sh
cargo check -p xtask
cargo clippy -p xtask --all-targets -- -D warnings
cargo nextest run -p xtask
cargo run -p xtask -- --help
```

For repository-wide gate changes, edit `taskit.toml`, inspect `cargo xtask <group> --help`, and run
the narrow gate before `cargo xtask check ci`.
