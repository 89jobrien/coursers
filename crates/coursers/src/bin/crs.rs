//! Entry point for the `crs` alias of the shared Coursers CLI.

use clap::Parser;
use coursers::Cli;

fn main() {
    let cli = Cli::parse();
    coursers::run(cli);
}
