//! Self-updating xtask shim that delegates to the `taskit` binary.
//!
//! Usage: `cargo xtask <subcommand> [args...]`
//!
//! If `taskit` is not installed, this shim installs it automatically
//! via `cargo install taskit`.

use std::process::{Command, exit};

/// Exit code to use when a child process exited without a code
/// (e.g. terminated by a signal on Unix).
const FALLBACK_EXIT_CODE: i32 = 1;

/// Compute the process exit code from a child status's reported code,
/// falling back to [`FALLBACK_EXIT_CODE`] when unavailable.
fn resolve_exit_code(code: Option<i32>) -> i32 {
    code.unwrap_or(FALLBACK_EXIT_CODE)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Try running taskit directly first
    match Command::new("taskit").args(&args).status() {
        Ok(status) => exit(resolve_exit_code(status.code())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("taskit not found, installing via cargo install...");
            let install = Command::new("cargo")
                .args(["install", "taskit"])
                .status()
                .expect("failed to run cargo install");
            if !install.success() {
                eprintln!("failed to install taskit");
                exit(FALLBACK_EXIT_CODE);
            }
            // Retry after install
            let status = Command::new("taskit")
                .args(&args)
                .status()
                .expect("failed to run taskit after install");
            exit(resolve_exit_code(status.code()));
        }
        Err(e) => {
            eprintln!("failed to run taskit: {e}");
            exit(FALLBACK_EXIT_CODE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_exit_code_passes_through_present_code() {
        assert_eq!(resolve_exit_code(Some(0)), 0);
        assert_eq!(resolve_exit_code(Some(42)), 42);
        assert_eq!(resolve_exit_code(Some(-1)), -1);
    }

    #[test]
    fn resolve_exit_code_falls_back_when_none() {
        assert_eq!(resolve_exit_code(None), FALLBACK_EXIT_CODE);
    }
}
