mod hook;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "coursers", about = "Claude Code course-correction hook pipeline")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// PreToolUse hook — reads JSON payload from stdin, writes hook response to stdout
    Pre,
    /// PostToolUse hook — reads JSON payload from stdin, records failures
    Post,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Pre => hook::pre::run(),
        Command::Post => hook::post::run(),
    }
}
