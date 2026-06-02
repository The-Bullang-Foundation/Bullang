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

    let update_handle = match cli.command {
        Command::Update => None,
        _ => Some(std::thread::spawn(|| {
            let remote = cmd::remote_head(cmd::DEFAULT_REPO, "main")?;
            let installed = cmd::installed_hash("bullang", cmd::DEFAULT_REPO, "main")?;
            if installed == remote {
                None // Pas de message si déjà à jour
            } else {
                Some(format!(
                    "\nA new version of bullang is available. Run `bullang update` to install."
                ))
            }
        })),
    };

    match cli.command {
        Command::Stdlib => cmd::cmd_stdlib(),
        Command::Update => cmd::cmd_update(),
    }

    if let Some(handle) = update_handle {
        if let Ok(Some(msg)) = handle.join() {
            println!("{}", msg);
        }
    }
}
