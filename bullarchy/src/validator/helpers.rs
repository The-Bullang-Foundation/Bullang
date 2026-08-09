//! File-system helpers and direct single-file validation.

use std::path::{Path, PathBuf};
use std::collections::HashSet;
use std::fs;
use bullang::ast::*;
use bullang::parser;


// ── Child callable collection ─────────────────────────────────────────────────

/// Functions a folder may call, split by language region.
///
/// A subtree declaring its own `#lang` is a separate region (decision 13), and
/// each region is transpiled to a different language in its own directory.
/// Calling across that boundary would need FFI, which Bullang does not
/// generate — so such a call is an error, and it needs to say so rather than
/// report the function as missing.
#[derive(Default)]
pub struct Callable {
    /// Declared in this region — callable.
    pub here: HashSet<String>,
    /// Declared in a nested region, with the folder that starts it.
    pub other_region: std::collections::HashMap<String, PathBuf>,
}

impl Callable {
    pub fn is_empty(&self) -> bool {
        self.here.is_empty() && self.other_region.is_empty()
    }
    pub fn contains(&self, name: &str) -> bool {
        self.here.contains(name)
    }
}

pub fn collect_child_callable(subdirs: &[PathBuf]) -> Callable {
    let mut out = Callable::default();
    for subdir in subdirs {
        collect_into(subdir, &mut out, None);
    }
    out
}

/// `region` is the folder that started a nested region, once one has been
/// entered — everything below it is out of reach for the caller.
fn collect_into(dir: &Path, out: &mut Callable, region: Option<&Path>) {
    let Ok(inv) = read_inventory(dir) else { return };

    // A folder declaring `#lang` starts a region of its own.
    let owned;
    let region = match (region, inv.lang.is_some()) {
        (Some(r), _)     => Some(r),
        (None, true)     => { owned = dir.to_path_buf(); Some(owned.as_path()) }
        (None, false)    => None,
    };

    for entry in &inv.entries {
        for func in &entry.functions {
            match region {
                Some(r) => { out.other_region.insert(func.clone(), r.to_path_buf()); }
                None    => { out.here.insert(func.clone()); }
            }
        }
    }
    for subdir in collect_subdirs(dir) {
        collect_into(&subdir, out, region);
    }
}

// ── Inventory / rank readers ──────────────────────────────────────────────────

pub fn read_inventory(dir: &Path) -> Result<InventoryFile, String> {
    let inv_path = dir.join("inventory.bu");
    let source   = crate::overlay::read_source(&inv_path)
        .map_err(|_| format!(
            "Missing inventory.bu in '{}' — every Bullang folder must have one.",
            dir.display()
        ))?;
    match parser::parse_file(&source, true) {
        Ok(BuFile::Inventory(inv)) => Ok(inv),
        Ok(_)  => Err(format!("inventory.bu in '{}' parsed as a source file.", dir.display())),
        Err(e) => Err(format!("Parse error in inventory.bu: {}", e)),
    }
}

pub fn read_folder_rank(dir: &Path) -> Option<Rank> {
    read_inventory(dir).ok().map(|inv| inv.rank)
}

// ── Path helpers ──────────────────────────────────────────────────────────────

pub fn main_bu_path(dir: &Path) -> Option<PathBuf> {
    let p = dir.join("main.bu");
    if p.exists() { Some(p) } else { None }
}

pub fn collect_bu_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter().flatten().flatten().map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension().map(|x| x == "bu").unwrap_or(false)
                && p.file_name().and_then(|n| n.to_str())
                    .map(|n| n != "inventory.bu" && n != "main.bu" && n != "blueprint.bu")
                    .unwrap_or(false)
        })
        .collect();
    files.sort();
    files
}

pub fn collect_subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter().flatten().flatten().map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}

// ── String helper ─────────────────────────────────────────────────────────────

pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None    => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}
