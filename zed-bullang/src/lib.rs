//! Zed extension for Bullang.
//!
//! Zed is the one editor `bullarchy editor-setup` cannot configure by writing a
//! file. Neovim, Vim, Helix and Emacs all accept "run this command for this
//! file type" as configuration; Zed requires an extension — a Rust crate
//! compiled to WebAssembly — before it will recognise a new language at all,
//! and that extension must name a tree-sitter grammar.
//!
//! The extension itself does almost nothing: it tells Zed where to find
//! `bullarchy` and to run it with `lsp`. The server, the diagnostics and the
//! grammar all live elsewhere. That is the whole of it.

use zed_extension_api::{self as zed, LanguageServerId, Result};

struct BullangExtension;

impl zed::Extension for BullangExtension {
    fn new() -> Self {
        BullangExtension
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        // `which` searches the worktree's PATH, which is the user's shell
        // environment rather than Zed's — that difference matters for a
        // cargo-installed binary in ~/.cargo/bin, which a GUI application
        // launched from a dock will not otherwise see.
        let path = worktree.which("bullarchy").ok_or_else(|| {
            "bullarchy was not found on PATH.\n\
             Install it with:\n  \
             cargo install --git https://github.com/The-Bullang-Foundation/Bullang.git \
             bullang bullarchy\n\
             then restart Zed."
                .to_string()
        })?;

        Ok(zed::Command {
            command: path,
            args: vec!["lsp".to_string()],
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(BullangExtension);
