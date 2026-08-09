//! The command line, as a declaration.
//!
//! This used to be hand-rolled: a `match` on `args[1]`, one bespoke parser per
//! subcommand, and `--help` and `--version` handled nowhere. The result drifted
//! from the documentation, and from the GUI:
//!
//!   - `convert -e <lang>` and `convert -o <file>` were documented and sent by
//!     the GUI's convert panel, and did not exist. `convert` read its second
//!     positional argument as *either* a language or an output path and guessed
//!     which, so `-e rs` was taken as a path named `-e`.
//!   - `fmt a b` silently kept `b` and discarded `a`, because the loop assigned
//!     over the same variable.
//!   - `--help` and `--version` were not recognised anywhere.
//!
//! clap was already a dependency and entirely unused. Declaring the surface
//! once means the parser, the help text and the GUI cannot disagree — and the
//! flags the GUI sends are now the flags that exist.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name    = "bullarchy",
    version,
    about   = "Bullang project toolchain",
    long_about = None,
    // Without arguments Bullarchy launches the GUI rather than printing help,
    // so no subcommand is not an error.
    arg_required_else_help = false,
)]
pub struct Cli {
    /// Launch the interactive terminal REPL.
    #[arg(long, global = false)]
    pub cli: bool,

    /// Launch the graphical interface (the default with no arguments).
    #[arg(long)]
    pub gui: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Scaffold a new Bullang project.
    Init {
        /// Project name. Must be a valid identifier.
        name: String,

        /// Hierarchy depth, 1 to 6.
        #[arg(long, default_value_t = 2)]
        depth: u8,

        /// Target language: rs, py, c, cpp, go or java.
        #[arg(long)]
        lang: Option<String>,

        /// A native header or import of the target language. Repeatable.
        #[arg(long)]
        lib: Vec<String>,

        /// Build the tree from a blueprint.bu file instead of --depth.
        #[arg(long)]
        blueprint: Option<PathBuf>,

        /// Where to create the project.
        #[arg(long)]
        path: Option<PathBuf>,
    },

    /// Transpile a project or a single .bu file.
    Convert {
        /// Project folder or .bu file. Defaults to the current directory.
        target: Option<PathBuf>,

        /// Language override: rs, py, c, cpp, go or java.
        ///
        /// Without it the language comes from the project's `#lang` directive.
        #[arg(short = 'e', long = "lang")]
        lang: Option<String>,

        /// Output file. Single-file mode only.
        #[arg(short = 'o', long = "out")]
        out: Option<PathBuf>,
    },

    /// Reformat .bu files to canonical style.
    Fmt {
        /// Format from this folder down. Defaults to the project root.
        folder: Option<PathBuf>,

        /// Report what would change without writing anything.
        #[arg(long = "dry-run")]
        dry_run: bool,
    },

    /// Validate and type-check the project.
    Check,

    /// Install a package, or list what is available.
    Add {
        /// Package name, or a git URL. Omit to list everything available.
        source: Option<String>,
    },

    /// Uninstall a package.
    Remove {
        /// Package name.
        name: String,
    },

    /// Write LSP configuration for Vim, Neovim, Helix and Emacs.
    EditorSetup,

    /// Reinstall Bullarchy from its repository.
    ///
    /// This used to run on every REPL start, on a thread that was joined
    /// immediately — so the check was synchronous despite looking otherwise,
    /// and every session paid for a network round trip it never asked for.
    Update,

    /// List the core standard library.
    Stdlib,

    /// Run the language server on stdio.
    Lsp,
}
