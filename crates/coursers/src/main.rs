mod hook;

use clap::{Parser, Subcommand};
use crs_core::config::ConfigBuilder;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "coursers",
    about = "Claude Code course-correction hook pipeline"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// PreToolUse hook — reads JSON payload from stdin, writes hook response to stdout
    Pre {
        /// Named profile (resolves to ~/.config/coursers/profiles/<name>/)
        #[arg(long)]
        profile: Option<String>,
        /// Override rules file path
        #[arg(long)]
        rules: Option<PathBuf>,
        /// Override global state file path
        #[arg(long)]
        state: Option<PathBuf>,
    },
    /// PostToolUse hook — reads JSON payload from stdin, records failures
    Post {
        /// Named profile (resolves to ~/.config/coursers/profiles/<name>/)
        #[arg(long)]
        profile: Option<String>,
        /// Override rules file path
        #[arg(long)]
        rules: Option<PathBuf>,
        /// Override global state file path
        #[arg(long)]
        state: Option<PathBuf>,
    },
}

fn build_profile(
    profile: Option<String>,
    rules: Option<PathBuf>,
    state: Option<PathBuf>,
) -> crs_core::config::ProfileConfig {
    let mut b = ConfigBuilder::new();
    if let Some(p) = profile {
        b = b.profile(p);
    }
    if let Some(r) = rules {
        b = b.rules(r);
    }
    if let Some(s) = state {
        b = b.state(s);
    }
    b.build()
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Pre {
            profile,
            rules,
            state,
        } => {
            let profile_cfg = build_profile(profile, rules, state);
            hook::pre::run_with_profile(&profile_cfg);
        }
        Command::Post {
            profile,
            rules,
            state,
        } => {
            let profile_cfg = build_profile(profile, rules, state);
            hook::post::run_with_profile(&profile_cfg);
        }
    }
}
