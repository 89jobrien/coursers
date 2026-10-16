//! Entry point for the `coursers` binary, including completion generation.

use clap::{CommandFactory, Parser};
use coursers::Cli;

// TODO(completions-discoverability): `completions` is intercepted before
// `Cli::parse`, so it is absent from `coursers --help`, `crs --help`, the
// README, and `docs/src/`. It is also unlisted in `Command`, so Clap's own
// subcommand metadata, shell-completion scripts, and `validate-hooks` never see
// it. The `crs` binary (src/bin/crs.rs) does not handle it at all, so
// `crs completions` falls through to normal Clap parsing and errors.
//
// Fix by modelling it as a real `Command::Completions` variant (generate for
// both shells via `clap_complete`'s factory) rather than a pre-parse string
// check, so it self-documents and stays reachable from `crs`.
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
