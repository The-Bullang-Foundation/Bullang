//! Shared utility functions used across command modules.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use crate::validator::{self, AllErrors};
use crate::typecheck;

// ── Directory helpers ─────────────────────────────────────────────────────────

pub fn current_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|e| {
        eprintln!("error: {}", e); std::process::exit(1);
    })
}

// ── Root detection ────────────────────────────────────────────────────────────

pub fn read_file(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error reading {}: {}", path.display(), e); std::process::exit(1);
    })
}

/// Like find_root_from but returns the given dir if no inventory found (no exit).
/// Walk up to the project root, without erroring if there is not one.
pub fn find_root_from_probe(start: &Path) -> PathBuf {
    if !start.join("inventory.bu").exists() { return start.to_path_buf(); }
    climb_to_root(start)
}

/// The outermost folder that is still part of this project.
///
/// "Outermost" used to mean the topmost ancestor with an `inventory.bu` — and
/// nothing more. Two Bullang projects side by side under a shared parent that
/// happened to have an inventory of its own were treated as one project, and
/// commands run inside either operated on both. The rank is what settles it:
/// climbing stops as soon as the parent stops looking like a folder of this
/// project.
fn climb_to_root(start: &Path) -> PathBuf {
    let mut root = start.to_path_buf();
    loop {
        let Some(parent) = root.parent() else { break };
        if !parent.join("inventory.bu").exists() {
            break;
        }
        // A parent whose inventory has no readable rank is not part of this
        // project's hierarchy, whatever else it may be.
        if validator::read_folder_rank(parent).is_none() {
            break;
        }
        root = parent.to_path_buf();
    }
    root
}

pub fn find_root_from(start: &Path) -> PathBuf {
    if !start.join("inventory.bu").exists() {
        eprintln!(
            "error: no inventory.bu in '{}'\n\
             run bullarchy from inside a Bullang project folder",
            start.display()
        );
        std::process::exit(1);
    }
    let root = climb_to_root(start);
    if validator::read_folder_rank(&root).is_none() {
        eprintln!("error: could not read #rank from '{}/inventory.bu'", root.display());
        std::process::exit(1);
    }
    root
}

// ── Error display ─────────────────────────────────────────────────────────────

pub fn print_all_errors(all: &AllErrors) {
    let mut by_file: BTreeMap<String, Vec<(usize, usize, String)>> = BTreeMap::new();

    for e in &all.parse {
        by_file.entry(e.file.clone()).or_default()
            .push((e.line, e.col, format!("parse error: {}", e.message)));
    }
    for e in &all.structural {
        by_file.entry(e.file.clone()).or_default()
            .push((e.line, e.col, e.message.clone()));
    }

    let mut total = 0;
    let file_count = by_file.len();

    for (file, mut entries) in by_file {
        entries.sort_by_key(|(line, col, _)| (*line, *col));
        eprintln!();
        eprintln!("  {}:", file);
        for (line, col, msg) in &entries {
            total += 1;
            if *line > 0 { eprintln!("    [{}:{}] {}", line, col, msg); }
            else         { eprintln!("    {}", msg); }
        }
    }

    eprintln!();
    eprintln!("{} error(s) in {} file(s)", total, file_count);
}

pub fn print_type_errors(errors: &[typecheck::TypeError]) {
    let mut by_file: BTreeMap<String, Vec<(usize, usize, String)>> = BTreeMap::new();

    for e in errors {
        by_file.entry(e.file.clone()).or_default()
            .push((e.line, e.col, e.message.clone()));
    }

    let mut total = 0;
    let file_count = by_file.len();

    for (file, mut entries) in by_file {
        entries.sort_by_key(|(line, col, _)| (*line, *col));
        eprintln!();
        eprintln!("  {}:", file);
        for (line, col, msg) in &entries {
            total += 1;
            if *line > 0 { eprintln!("    [{}:{}] type error: {}", line, col, msg); }
            else         { eprintln!("    type error: {}", msg); }
        }
    }

    eprintln!();
    eprintln!("{} type error(s) in {} file(s)", total, file_count);
}

// ── Generated-source formatting ─────────────────────────────────────────────

/// Run the appropriate code formatter on a generated source file, keyed by
/// its extension. Shared by `build` and `convert` so both stay in sync.
/// Falls back to unformatted content if the formatter isn't installed or
/// fails — but tells the user why on stderr, rather than silently leaving
/// them with unreadable one-line output and no explanation.
pub fn format_source(path: &Path, content: &str) -> Option<String> {
    let ext = path.extension()?.to_str()?;
    match ext {
        "rs" => run_formatter(&["rustfmt", "--edition", "2024"], content),
        "py" => run_formatter(&["black", "-q", "-"], content),
        "go" => run_formatter(&["gofmt"], content),
        "c" | "cpp" | "h" | "hpp" => run_formatter(&["clang-format", "--style=LLVM"], content),
        "java" => run_formatter(&["google-java-format", "-"], content),
        _ => None,
    }
}

fn run_formatter(cmd: &[&str], content: &str) -> Option<String> {
    use std::io::Write;

    let mut child = match std::process::Command::new(cmd[0])
        .args(&cmd[1..])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => {
            eprintln!(
                "warning: `{}` not found — generated output will be unformatted \
                 (install it for readable output)",
                cmd[0]
            );
            return None;
        }
    };

    child.stdin.take()?.write_all(content.as_bytes()).ok()?;
    let output = child.wait_with_output().ok()?;

    if output.status.success() && !output.stdout.is_empty() {
        String::from_utf8(output.stdout).ok()
    } else {
        eprintln!(
            "warning: `{}` failed to format generated output — leaving it unformatted",
            cmd[0]
        );
        None
    }
}

// ── HTTP ──────────────────────────────────────────────────────────────────────

/// Fetch a URL, or `None` if it could not be read.
///
/// This used to shell out to `curl`, which was never declared as a dependency
/// anywhere — not in the README, not checked at startup — so on a machine
/// without it `bullarchy add` simply reported that the registry was
/// unreachable, which is a different problem with a different fix.
///
/// `ureq` was already in `Cargo.toml` and entirely unused. Doing it in-process
/// removes the dependency rather than documenting it.
pub fn fetch_url(url: &str) -> Option<String> {
    ureq::get(url)
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .ok()?
        .into_string()
        .ok()
}
