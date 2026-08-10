mod build;
mod cli;
mod cmd;
mod codegen;
mod overlay;
mod pipe;
mod sanitize;
mod init;
mod lsp;
mod stdlib;
mod typecheck;
mod utils;
mod validator;

use clap::Parser;
use cli::{Cli, Command};

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Lsp) => lsp::run(),
        Some(cmd)          => run_command(cmd),
        // No subcommand: --cli asks for the REPL, everything else launches the
        // GUI. `--help` and `--version` never reach here — clap handles both,
        // which is where they were missing entirely before.
        None if cli.cli    => run_cli_repl(),
        None               => launch_gui(),
    }
}

fn run_command(cmd: Command) {
    match cmd {
        Command::Init { name, depth, lang, lib, blueprint, path } =>
            cmd::cmd_init(name, depth, blueprint, lang, lib, path),
        Command::Convert { target, lang, out } =>
            cmd::cmd_convert(target, lang, out),
        Command::Fmt { folder, dry_run } =>
            cmd::cmd_fmt(folder, dry_run),
        Command::Check        => cmd::cmd_check(),
        Command::Add { source } => match source {
            Some(s) => cmd::cmd_add(&[s.as_str()]),
            None    => cmd::cmd_add(&[]),
        },
        Command::Remove { name } => cmd::cmd_remove(&[name.as_str()]),
        Command::EditorSetup  => cmd::cmd_editor_setup(),
        Command::Update       => cmd::cmd_update(),
        Command::Stdlib       => print_stdlib(),
        // Handled in main, before the GUI/REPL decision.
        Command::Lsp          => lsp::run(),
    }
}

/// The core standard library, by category.
fn print_stdlib() {
    use bullang::stdlib::{by_category, Category};
    println!();
    for category in Category::ALL {
        let entries: Vec<_> = by_category(*category).collect();
        if entries.is_empty() {
            continue;
        }
        println!("  {}", category.title());
        for b in entries {
            println!("    {:<52} {}", b.signature, b.description);
        }
        println!();
    }
}

// ── GUI launcher ──────────────────────────────────────────────────────────────

fn launch_gui() {
    // Look for bullarchy-gui in the same locations the GUI looks for bullarchy
    let candidates = vec![
        // Same directory as this binary
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("bullarchy-gui")))
            .unwrap_or_default(),
        // ~/.local/bin (Linux)
        {
            let home = std::env::var("HOME").unwrap_or_default();
            std::path::PathBuf::from(&home).join(".local").join("bin").join("bullarchy-gui")
        },
        // /usr/local/bin (macOS)
        std::path::PathBuf::from("/usr/local/bin/bullarchy-gui"),
        // Windows: %USERPROFILE%\AppData\Local\Programs\bullarchy-gui.exe
        {
            let home = std::env::var("USERPROFILE").unwrap_or_default();
            std::path::PathBuf::from(&home)
                .join("AppData").join("Local").join("Programs")
                .join("bullarchy-gui.exe")
        },
    ];

    for path in &candidates {
        if path.exists() {
            match spawn_detached(path) {
                Ok(_)  => return,
                Err(e) => eprintln!("  Failed to launch GUI: {}", e),
            }
        }
    }

    // GUI not found — fall back to CLI REPL with a hint
    eprintln!("  bullarchy-gui not found. Launching CLI instead.");
    eprintln!("  To install the GUI, run the Bullang installer:");
    eprintln!("  https://github.com/The-Bullang-Foundation/bullang-installer");
    eprintln!();
    run_cli_repl();
}

/// Start the GUI as an independent process and return.
///
/// A plain `spawn()` leaves the child holding this terminal. It inherits
/// stdin, stdout and stderr, and — more importantly — it stays in the shell's
/// foreground process group even after `bullarchy` itself exits. The terminal
/// therefore stays occupied by a window that has nothing to say to it, and
/// Ctrl+C is what frees it, because that signals the whole process group.
///
/// So: no inherited streams, and a process group of its own. The GUI reports
/// in its window; it has no use for a terminal.
fn spawn_detached(path: &std::path::Path) -> std::io::Result<std::process::Child> {
    use std::process::Stdio;

    let mut cmd = std::process::Command::new(path);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(unix)]
    {
        // A new process group, so Ctrl+C in the terminal that launched it does
        // not reach the GUI and the shell does not count it as part of the job.
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    #[cfg(windows)]
    {
        // DETACHED_PROCESS: no console is inherited or created.
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0000_0008);
    }

    cmd.spawn()
}

// ── CLI REPL ──────────────────────────────────────────────────────────────────

fn run_cli_repl() {
    println!("{}", BANNER);

    // The update check that used to live here ran on a thread that was joined
    // immediately, so it was synchronous despite looking otherwise and every
    // session paid for a network round trip nobody asked for. It is now the
    // explicit `bullarchy update`.

    let mut rl = rustyline::DefaultEditor::new()
        .expect("failed to initialise line editor");

    loop {
        let line = match rl.readline("command -> ") {
            Ok(l)                                              => l,
            Err(rustyline::error::ReadlineError::Eof)         => { println!("Goodbye."); break; }
            Err(rustyline::error::ReadlineError::Interrupted) => continue,
            Err(e) => { eprintln!("Read error: {}", e); break; }
        };
        let line = line.trim();
        if line.is_empty() { continue; }
        let _ = rl.add_history_entry(line);

        if line == "exit" {
            println!("Goodbye.");
            break;
        }

        // Parsed by clap, exactly as a shell invocation would be — so the REPL
        // accepts precisely the commands and flags the CLI does, and its help
        // is the same help.
        let argv = std::iter::once("bullarchy").chain(line.split_whitespace());
        match Cli::try_parse_from(argv) {
            Ok(parsed) => match parsed.command {
                Some(cmd) => run_command(cmd),
                None      => print_repl_help(),
            },
            Err(e) => { let _ = e.print(); }
        }
    }
}

/// The REPL's own help. `bullarchy help` and `--help` are clap's.
fn print_repl_help() {
    println!();
    println!("  Type a command exactly as you would on the command line.");
    println!("  '--help' lists them; '<command> --help' explains one.");
    println!("  'exit' quits.");
    println!();
}

// ── Banner ────────────────────────────────────────────────────────────────────

const BANNER: &str = r#"
 ____        _ _               _
|  _ \      | | |             | |
| |_) |_   _| | | __ _ _ __ ___| |__  _   _
|  _ <| | | | | |/ _` | '__/ __| '_ \| | | |
| |_) | |_| | | | (_| | | | (__| | | | |_| |
|____/ \__,_|_|_|\__,_|_|  \___|_| |_|\__, |
                                        __/ |
                                       |___/

Bullarchy — Bullang project toolchain
Type 'help' for available commands. Type 'exit' to quit.
"#;
