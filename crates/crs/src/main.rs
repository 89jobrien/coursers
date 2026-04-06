use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "crs", about = "Command rewriter and output filter")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Rewrite a command to its crs equivalent (exit 0 = rewritten, exit 1 = passthrough)
    Rewrite {
        /// The raw command to rewrite
        command: String,
    },
    /// Discover missed savings from Claude Code history
    Discover {
        /// Scan all projects
        #[arg(short, long)]
        all: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Rewrite { command } => {
            eprintln!("rewrite: {command} (not yet implemented)");
            std::process::exit(1);
        }
        Command::Discover { all } => {
            eprintln!("discover (all={all}): not yet implemented");
        }
    }
}
