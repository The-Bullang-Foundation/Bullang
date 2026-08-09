//! `bullarchy fmt` — formats all .bu files in a project to canonical style.
//!
//! Rewrites in place. Escape block contents are never touched.
//! Run `bullarchy check` to verify formatting without writing.

use std::path::{Path, PathBuf};
use std::fs;

use bullang::ast::BuFile;
use bullang::fmt;
use bullang::parser;
use crate::utils::current_dir;
use crate::validator::{collect_bu_files, collect_subdirs};

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn cmd_fmt(folder: Option<PathBuf>, dry_run: bool) {
    let source_dir = match folder {
        Some(ref p) => p.canonicalize().unwrap_or_else(|_| p.clone()),
        None        => current_dir(),
    };
    // Format from the given folder down.
    //
    // This used to walk up to the project root and format the *whole* project
    // regardless of the argument, so `bullarchy fmt src/parser` rewrote every
    // file in the repository. The argument named a folder and then did
    // nothing — worse than not accepting one.
    if !source_dir.is_dir() {
        eprintln!("error: '{}' is not a directory", source_dir.display());
        std::process::exit(1);
    }

    let verb = if dry_run { "would reformat" } else { "reformatted" };

    let mut changed = 0usize;
    let mut total   = 0usize;

    format_tree(&source_dir, dry_run, verb, &mut total, &mut changed);

    println!();
    if changed == 0 {
        println!("{} file(s) checked — already formatted.", total);
    } else if dry_run {
        println!("{} file(s) would be reformatted (run `bullarchy fmt` to apply).", changed);
    } else {
        println!("{} file(s) reformatted, {} unchanged.", changed, total - changed);
    }
}

// ── Tree walker ───────────────────────────────────────────────────────────────

fn format_tree(
    dir:     &Path,
    dry_run: bool,
    verb:    &str,
    total:   &mut usize,
    changed: &mut usize,
) {
    // Format inventory.bu for this folder
    let inv_path = dir.join("inventory.bu");
    if inv_path.exists() {
        format_file(&inv_path, true, dry_run, verb, total, changed);
    }

    // Format all source .bu files
    for bu_path in collect_bu_files(dir) {
        format_file(&bu_path, false, dry_run, verb, total, changed);
    }

    // Format main.bu if present
    let main_path = dir.join("main.bu");
    if main_path.exists() {
        format_file(&main_path, false, dry_run, verb, total, changed);
    }

    // Recurse
    for subdir in collect_subdirs(dir) {
        format_tree(&subdir, dry_run, verb, total, changed);
    }
}

// ── Single file formatter ─────────────────────────────────────────────────────

fn format_file(
    path:         &Path,
    is_inventory: bool,
    dry_run:      bool,
    verb:         &str,
    total:        &mut usize,
    changed:      &mut usize,
) {
    *total += 1;

    let source = match fs::read_to_string(path) {
        Ok(s)  => s,
        Err(e) => { eprintln!("warning: could not read {}: {}", path.display(), e); return; }
    };

    let formatted = match parser::parse_file(&source, is_inventory) {
        Ok(BuFile::Source(ref sf))    => fmt::format_source(sf),
        Ok(BuFile::Inventory(ref inv)) => fmt::format_inventory(inv),
        Err(e) => {
            eprintln!("  skip  {} (parse error: {})", path.display(), e);
            return;
        }
    };

    if formatted == source {
        return; // already canonical
    }

    *changed += 1;
    println!("  {}  {}", verb, path.display());

    if !dry_run {
        if let Err(e) = fs::write(path, &formatted) {
            eprintln!("warning: could not write {}: {}", path.display(), e);
        }
    }
}

// ── Format check (used by bullang check) ──────────────────────────────────────

/// Walk the tree and return paths of all .bu files that are not in canonical format.
/// Called by `cmd_check` — no files are written.
pub fn check_formatting(root: &Path) -> Vec<PathBuf> {
    let mut unformatted = Vec::new();
    check_tree(root, &mut unformatted);
    unformatted
}

fn check_tree(dir: &Path, out: &mut Vec<PathBuf>) {
    let inv_path = dir.join("inventory.bu");
    if inv_path.exists() {
        check_file(&inv_path, true, out);
    }
    for bu_path in collect_bu_files(dir) {
        check_file(&bu_path, false, out);
    }
    let main_path = dir.join("main.bu");
    if main_path.exists() {
        check_file(&main_path, false, out);
    }
    for subdir in collect_subdirs(dir) {
        check_tree(&subdir, out);
    }
}

fn check_file(path: &Path, is_inventory: bool, out: &mut Vec<PathBuf>) {
    let source = match fs::read_to_string(path) {
        Ok(s)  => s,
        Err(_) => return,
    };
    let formatted = match parser::parse_file(&source, is_inventory) {
        Ok(BuFile::Source(ref sf))     => fmt::format_source(sf),
        Ok(BuFile::Inventory(ref inv)) => fmt::format_inventory(inv),
        Err(_) => return, // parse errors reported by validator, not formatter
    };
    if formatted != source {
        out.push(path.to_path_buf());
    }
}
