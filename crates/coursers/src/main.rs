use clap::{CommandFactory, Parser};
use coursers::Cli;

fn main() {
    if std::env::args().nth(1).as_deref() == Some("completions") {
        clap_complete::generate(
            clap_complete_nushell::Nushell,
            &mut Cli::command(),
            "coursers",
            &mut std::io::stdout(),
        );
        return;
    }

    let cli = Cli::parse();
    coursers::run(cli);
}
