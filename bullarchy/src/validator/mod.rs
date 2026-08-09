//! Compile-time structural validation.
//!
//! Uses tolerant parsing: one broken function does not abort validation
//! of the rest of the file. All errors across all files are collected
//! before returning, so the developer sees the full picture in one run.

pub mod helpers;
mod inventory;
pub mod source;

pub use helpers::{
    read_inventory, read_folder_rank,
    collect_bu_files, collect_subdirs,
    main_bu_path,
};

use std::path::Path;
use bullang::ast::*;
use bullang::parser;

// ── Error types ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ValidationError {
    pub file:    String,
    pub line:    usize,
    pub col:     usize,
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line > 0 {
            write!(f, "[{}:{}:{}] {}", self.file, self.line, self.col, self.message)
        } else {
            write!(f, "[{}] {}", self.file, self.message)
        }
    }
}

/// All errors from one validation run — parse errors and structural errors
/// kept together so they can be sorted and displayed uniformly.
#[derive(Debug)]
pub struct AllErrors {
    pub parse:      Vec<bullang::parser::ParseError>,
    pub structural: Vec<ValidationError>,
}

impl AllErrors {
    pub fn new() -> Self { Self { parse: vec![], structural: vec![] } }
    pub fn is_empty(&self) -> bool { self.parse.is_empty() && self.structural.is_empty() }
    pub fn push_structural(&mut self, e: ValidationError) { self.structural.push(e); }
    pub fn extend_structural(&mut self, es: Vec<ValidationError>) { self.structural.extend(es); }
    pub fn extend_parse(&mut self, es: Vec<bullang::parser::ParseError>) { self.parse.extend(es); }
    pub fn extend_all(&mut self, other: AllErrors) {
        self.parse.extend(other.parse);
        self.structural.extend(other.structural);
    }
}

// ── Error constructors ────────────────────────────────────────────────────────

pub(crate) fn err(path: &Path, msg: impl Into<String>) -> ValidationError {
    ValidationError { file: path.display().to_string(), line: 0, col: 0, message: msg.into() }
}

fn ferr(file: &str, msg: impl Into<String>) -> ValidationError {
    ValidationError { file: file.to_string(), line: 0, col: 0, message: msg.into() }
}

// ── Public entry point ────────────────────────────────────────────────────────

pub fn validate_tree(root: &Path) -> AllErrors {
    let mut all = validate_folder(root, None);
    all.extend_structural(validate_main_placement(root, true));
    all
}

/// `main.bu` belongs at the root of its language region, and nowhere else.
///
/// Nothing checked this, and the Rust backend's `Cargo.toml` assumed it: it
/// wrote `[[bin]] path = "src/main.rs"` whenever a `main.bu` existed *anywhere*
/// in the tree, while the file itself was generated beside its own folder. A
/// project with `a/main.bu` therefore produced a manifest pointing at a
/// `src/main.rs` that was never written, and `cargo build` failed on generated
/// code the user had no reason to suspect.
///
/// A region root is the project root, or any folder declaring its own `#lang`
/// (decision 13). Each region is self-contained and gets its own entry point.
fn validate_main_placement(dir: &Path, is_region_root: bool) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if !is_region_root {
        if let Some(mp) = helpers::main_bu_path(dir) {
            errors.push(ValidationError {
                file: mp.display().to_string(),
                line: 0, col: 0,
                message:
                    "main.bu must sit at the root of its language region. A region \
                     is the project root, or any folder with its own '#lang'. Move \
                     it up, or give this folder a '#lang' of its own to make it a \
                     region."
                        .to_string(),
            });
        }
    }

    for subdir in helpers::collect_subdirs(dir) {
        // A child declaring `#lang` starts a region of its own, so it may have
        // its own main.bu.
        let starts_region = helpers::read_inventory(&subdir)
            .map(|inv| inv.lang.is_some())
            .unwrap_or(false);
        errors.extend(validate_main_placement(&subdir, starts_region));
    }
    errors
}

// ── Folder validation (recursive, bottom-up) ─────────────────────────────────

fn validate_folder(dir: &Path, parent_lang: Option<&bullang::ast::Backend>) -> AllErrors {
    let mut all = AllErrors::new();

    let inv = match helpers::read_inventory(dir) {
        Ok(i)  => i,
        Err(e) => {
            all.push_structural(err(dir, e));
            return all;
        }
    };

    // A subtree may declare its own `#lang` — that is what a language region
    // is (decision 13). The check that used to live here rejected any child
    // whose `#lang` differed from its parent's, which made regions impossible
    // to express. What replaces it is the rule that a call may not cross a
    // region boundary; that is checked where calls are, not here.
    let _ = dir.join("inventory.bu");

    // The effective language is the nearest ancestor's that declares one —
    // this folder's if it has one, else whatever was inherited.
    let effective_lang = inv.lang.as_ref().or(parent_lang);

    let subdirs   = helpers::collect_subdirs(dir);
    let bu_files  = helpers::collect_bu_files(dir);
    let main_path = helpers::main_bu_path(dir);

    // Recurse into sub-folders (bottom-up), passing effective lang down
    for subdir in &subdirs {
        all.extend_all(validate_folder(subdir, effective_lang));
    }

    match inv.rank {
        // ── War ───────────────────────────────────────────────────────────────
        Rank::War => {
            if !bu_files.is_empty() {
                all.push_structural(err(dir, format!(
                    "War folder cannot contain source files (found {}). \
                     Consider using a theater rank instead.",
                    bu_files.len()
                )));
            }
            if subdirs.len() > 5 {
                all.push_structural(err(dir, format!(
                    "War folder cannot exceed 5 theaters (found {}).",
                    subdirs.len()
                )));
            }
            if !inv.entries.is_empty() {
                all.push_structural(err(
                    &dir.join("inventory.bu"),
                    "War inventory cannot list any files."
                ));
            }
            for subdir in &subdirs {
                validate_child_rank(subdir, &Rank::Theater, &mut all);
            }
            if let Some(ref mp) = main_path {
                let child_callable = helpers::collect_child_callable(&subdirs);
                all.extend_all(validate_main_file(mp, &child_callable));
            }
        }

        // ── Skirmish ──────────────────────────────────────────────────────────
        Rank::Skirmish => {
            if !subdirs.is_empty() {
                all.push_structural(err(dir, format!(
                    "Skirmish folder cannot contain sub-folders (found {}).",
                    subdirs.len()
                )));
            }
            if bu_files.len() > 5 {
                all.push_structural(err(dir, format!(
                    "Skirmish folder cannot contain more than 5 source files (found {}).",
                    bu_files.len()
                )));
            }
            if main_path.is_some() {
                all.push_structural(err(
                    &dir.join("main.bu"),
                    "Skirmish folders cannot contain main.bu. \
                     Move your entry point to a tactic or higher rank folder."
                ));
            }
            all.extend_structural(inventory::validate_inventory_structs(dir, &inv, &[]));
            all.extend_structural(inventory::validate_inventory_completeness(
                dir, &inv, &bu_files, &[],
            ));
            let inv_map = inventory::build_inv_map(&inv);
            for bu in &bu_files {
                all.extend_all(source::validate_source_file(
                    bu, &inv.rank, &inv_map, &Default::default(), effective_lang,
                ));
            }
        }

        // ── Middle ranks ──────────────────────────────────────────────────────
        ref rank => {
            let child_rank = rank.child_rank().unwrap();

            if subdirs.len() > 5 {
                all.push_structural(err(dir, format!(
                    "{} folder cannot contain more than 5 {} sub-folders (found {}).",
                    helpers::capitalize(rank.name()), child_rank.name(), subdirs.len()
                )));
            }
            if bu_files.len() > 5 {
                all.push_structural(err(dir, format!(
                    "{} folder cannot contain more than 5 source files (found {}).",
                    helpers::capitalize(rank.name()), bu_files.len()
                )));
            }
            for subdir in &subdirs {
                validate_child_rank(subdir, &child_rank, &mut all);
            }
            all.extend_structural(inventory::validate_inventory_structs(dir, &inv, &subdirs));
            all.extend_structural(inventory::validate_inventory_completeness(
                dir, &inv, &bu_files, &subdirs,
            ));
            let child_callable = helpers::collect_child_callable(&subdirs);
            let inv_map        = inventory::build_inv_map(&inv);
            for bu in &bu_files {
                all.extend_all(source::validate_source_file(bu, rank, &inv_map, &child_callable, effective_lang));
            }
            if let Some(ref mp) = main_path {
                all.extend_all(validate_main_file(mp, &child_callable));
            }
        }
    }

    all
}

fn validate_child_rank(subdir: &Path, expected: &Rank, all: &mut AllErrors) {
    match helpers::read_folder_rank(subdir) {
        Some(ref actual) if actual == expected => {}
        Some(ref actual) => {
            all.push_structural(err(subdir, format!(
                "Found unexpected '{}' in inventory. Consider replacing it with '{}'.",
                actual.name(), expected.name()
            )));
        }
        None => {
            all.push_structural(err(subdir, format!(
                "Sub-folder '{}' is missing inventory.bu (expected a {} folder).",
                subdir.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                expected.name()
            )));
        }
    }
}

// ── main.bu validation ────────────────────────────────────────────────────────

/// `main` takes no parameters.
///
/// Every backend's `main` has a fixed signature the target language dictates,
/// so declared parameters were silently dropped — the function compiled and
/// the values were simply never passed. Command-line input comes from
/// `builtin::args`, which is why it exists.
fn check_main_signature(func: &bullang::ast::Bullet, path: &str) -> Vec<ValidationError> {
    if func.name != "main" || func.params.is_empty() {
        return vec![];
    }
    vec![ValidationError {
        file: path.to_string(),
        line: func.span.line,
        col:  func.span.col,
        message: format!(
            "'main' cannot take parameters (found {}). Every target language \
             fixes main's signature, so these would be silently dropped. Read \
             command-line input with `builtin::args` and `builtin::argc`.",
            func.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
        ),
    }]
}

fn validate_main_file(path: &Path, callable: &helpers::Callable) -> AllErrors {
    let mut all = AllErrors::new();

    let src = match crate::overlay::read_source(path) {
        Ok(s)  => s,
        Err(e) => {
            all.push_structural(err(path, format!("Could not read main.bu: {}", e)));
            return all;
        }
    };

    let path_str = path.display().to_string();
    let result   = parser::parse_file_tolerant(&src, &path_str);
    all.extend_parse(result.errors);

    if let BuFile::Source(ref sf) = result.file {
        if sf.bullets.len() > 5 {
            all.push_structural(ferr(&path_str, format!(
                "main.bu cannot contain more than 5 functions (found {}).",
                sf.bullets.len()
            )));
        }
        for func in &sf.bullets {
            all.extend_structural(check_main_signature(func, &path_str));
            all.extend_structural(source::validate_function(func, &path_str, callable, false));
        }
    }

    all
}
