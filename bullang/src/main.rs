mod cmd;

use clap::{Parser as ClapParser, Subcommand};

#[derive(ClapParser)]
#[command(
    name    = "bullang",
    version = env!("CARGO_PKG_VERSION"),
    about   = "Bullang — the language definition.\n\n\
               Defines the .bu language: grammar, parser, AST, formatter, and the\n\
               core standard library catalogue. Bullang describes the language; it\n\
               does not run it. For transpiling, formatting, scaffolding, package\n\
               management and LSP support, use bullarchy."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Browse the core standard library.
    ///
    /// Example:
    ///
    ///   bullang stdlib
    Stdlib,
}

fn main() {
    match Cli::parse().command {
        Command::Stdlib => cmd::cmd_stdlib(),
    }
}
