use std::process::{Command, ExitCode};

fn run(program: &str, args: &[&str]) -> Result<(), i32> {
    let status = Command::new(program)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn `{program}`: {e}"));
    if status.success() {
        Ok(())
    } else {
        Err(status.code().unwrap_or(1))
    }
}

fn cmd_build() -> Result<(), i32> {
    run("cargo", &["build", "--workspace"])
}

fn cmd_ci() -> Result<(), i32> {
    println!("=== fmt ===");
    run("cargo", &["fmt", "--all", "--check"])?;

    println!("=== clippy ===");
    run("cargo", &["clippy", "--workspace", "--", "-D", "warnings"])?;

    println!("=== test ===");
    run("cargo", &["nextest", "run", "--workspace"])
}

fn main() -> ExitCode {
    let subcommand = std::env::args().nth(1);
    let result = match subcommand.as_deref() {
        Some("build") => cmd_build(),
        Some("ci") => cmd_ci(),
        Some(other) => {
            eprintln!("error: unknown subcommand `{other}`");
            eprintln!("usage: cargo xtask <build|ci>");
            return ExitCode::FAILURE;
        }
        None => {
            eprintln!("error: subcommand required");
            eprintln!("usage: cargo xtask <build|ci>");
            return ExitCode::FAILURE;
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => ExitCode::from(code as u8),
    }
}
