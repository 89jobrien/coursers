use clap::Parser;
use coursers::Cli;

fn main() {
    let cli = Cli::parse();
    coursers::run(cli);
}
