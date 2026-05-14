//! `bullang prod` — strips test folders from a converted output tree.
//!
//! Deletes every folder whose name starts with `test_`, recursively.
//! Uses a restart-from-root strategy after each deletion: safe against
//! iterator invalidation and guarantees no test folder is missed even
//! when test folders are nested inside other test folders.

use std::path::{Path, PathBuf};
use std::fs;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn cmd_prod(folder: PathBuf) {
    let root = folder.canonicalize().unwrap_or_else(|_| folder.clone());

    if !root.exists() {
        eprintln!("error: '{}' does not exist", root.display());
        std::process::exit(1);
    }
    if !root.is_dir() {
        eprintln!("error: '{}' is not a directory", root.display());
        std::process::exit(1);
    }

    println!("bullang prod");
    println!("  root : {}", root.display());
    println!();

    let mut removed = 0usize;

    // Restart from root after every deletion until a full pass finds nothing
    loop {
        match find_and_remove_test_folder(&root) {
            Some(path) => {
                println!("  removed  {}", path.display());
                removed += 1;
                // Restart — the tree has changed
            }
            None => break, // full pass with no test_ folder found — done
        }
    }

    // Remove work_division.md if present at the root
    let wd_path = root.join("work_division.md");
    let removed_wd = if wd_path.exists() {
        match fs::remove_file(&wd_path) {
            Ok(_)  => { println!("  removed  {}", wd_path.display()); true }
            Err(e) => { eprintln!("warning: could not remove 'work_division.md': {}", e); false }
        }
    } else {
        false
    };

    println!();
    if removed == 0 && !removed_wd {
        println!("nothing to remove.");
    } else {
        if removed > 0 {
            println!("{} test_ folder(s) removed.", removed);
        }
        if removed_wd {
            println!("work_division.md removed.");
        }
    }
}

// ── Tree walker ───────────────────────────────────────────────────────────────

/// Walk the tree depth-first. Return the path of the first `test_` folder
/// found and delete it, or return `None` if none exists.
///
/// Depth-first ensures we hit the deepest test folders first, but since we
/// restart from root on every deletion this order doesn't matter for
/// correctness — it just feels natural.
fn find_and_remove_test_folder(dir: &Path) -> Option<PathBuf> {
    let entries = match fs::read_dir(dir) {
        Ok(e)  => e,
        Err(_) => return None,
    };

    let mut subdirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();

    subdirs.sort(); // deterministic order

    for subdir in subdirs {
        let name = subdir.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if name.starts_with("test_") {
            // Delete the folder and everything inside it
            if let Err(e) = fs::remove_dir_all(&subdir) {
                eprintln!("warning: could not remove '{}': {}", subdir.display(), e);
            }
            return Some(subdir);
        }

        // Recurse into non-test folders
        if let Some(found) = find_and_remove_test_folder(&subdir) {
            return Some(found);
        }
    }

    None
}
