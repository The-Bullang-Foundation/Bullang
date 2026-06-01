mod cmd;

use clap::{Parser as ClapParser, Subcommand};

#[derive(ClapParser)]
#[command(
    name    = "bullang",
    version = env!("CARGO_PKG_VERSION"),
    about   = "Bullang — the language registry.\n\n\
               Defines the .bu language: grammar, parser, AST, type system, and standard library.\n\
               For transpiling, formatting, scaffolding, and LSP support, use bullarchy."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Explore the standard library of builtin functions.
    ///
    /// Example:
    ///
    ///   bullang stdlib
    Stdlib,

    /// Update bullang to the latest version from the source repository.
    ///
    /// Requires git and cargo to be available on PATH.
    ///
    /// Example:
    ///
    ///   bullang update
    Update,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Stdlib => cmd::cmd_stdlib(),
        Command::Update => cmd::cmd_update(),
    }
}
