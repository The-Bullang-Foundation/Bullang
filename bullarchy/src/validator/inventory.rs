//! Inventory completeness checks: every file, function, and struct must be valid.

use std::path::{Path, PathBuf};
use std::collections::{HashSet, HashMap};
use bullang::ast::*;
use bullang::parser;
use super::ValidationError;

pub fn build_inv_map(inv: &InventoryFile) -> HashMap<String, Vec<String>> {
    inv.entries.iter()
        .map(|e| (e.file.clone(), e.functions.clone()))
        .collect()
}

// ── Struct validation ─────────────────────────────────────────────────────────

/// Validate structs declared in this inventory.
/// Rules:
/// - Every struct must have at least one field.
/// - Field types must be either known primitives, known generic types,
///   or structs declared in child folder inventories (enforcing the rank rule).
pub fn validate_inventory_structs(
    dir:            &Path,
    inv:            &InventoryFile,
    subdirs:        &[PathBuf],
) -> Vec<ValidationError> {
    let mut errors  = Vec::new();
    let inv_path    = dir.join("inventory.bu");
    let inv_str     = inv_path.display().to_string();

    // Collect struct names available from child inventories
    let child_struct_names: HashSet<String> = subdirs.iter()
        .filter_map(|sd| super::helpers::read_inventory(sd).ok())
        .flat_map(|child_inv| child_inv.structs.into_iter().map(|s| s.name))
        .collect();

    for s in &inv.structs {
        if s.fields.is_empty() {
            errors.push(ferr(&inv_str, format!(
                "Struct '{}' has no fields. Add at least one field.",
                s.name
            )));
            continue;
        }

        for field in &s.fields {
            if let BuType::Named(ref type_name) = field.ty {
                let base = base_type_name(type_name);
                if !is_known_primitive(base) && !child_struct_names.contains(base) {
                    // Check if it's a struct defined in *this* inventory
                    let in_self = inv.structs.iter().any(|st| st.name == base);
                    if !in_self {
                        errors.push(ferr(&inv_str, format!(
                            "Struct '{}': field '{}' has unknown type '{}'. \
                             To use a struct type, define it in a child folder's inventory.",
                            s.name, field.name, type_name
                        )));
                    }
                }
            }
        }
    }

    errors
}

/// Extract the base type name from a possibly-generic type string.
/// `Vec[Point]` → `Vec`, `Point` → `Point`, `HashMap[String, i32]` → `HashMap`
fn base_type_name(s: &str) -> &str {
    if let Some(bracket) = s.find('[') { &s[..bracket] } else { s }
}

fn is_known_primitive(s: &str) -> bool {
    matches!(s,
        "i8"|"i16"|"i32"|"i64"|"i128"|"isize"|
        "u8"|"u16"|"u32"|"u64"|"u128"|"usize"|
        "f32"|"f64"|"bool"|"char"|"String"|"str"|
        "Vec"|"Option"|"Tuple"|"Fn"|"Box"|"HashMap"
    )
}

// ── File/function completeness ─────────────────────────────────────────────────

pub fn validate_inventory_completeness(
    dir:      &Path,
    inv:      &InventoryFile,
    bu_files: &[PathBuf],
    _subdirs: &[PathBuf],
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let inv_path   = dir.join("inventory.bu");
    let inv_str    = inv_path.display().to_string();

    let file_stems: HashSet<String> = bu_files.iter()
        .filter_map(|p| p.file_stem()?.to_str().map(|s| s.to_string()))
        .collect();

    let inv_stems: HashSet<String> = inv.entries.iter()
        .map(|e| e.file.clone())
        .collect();

    for stem in &file_stems {
        if !inv_stems.contains(stem) {
            errors.push(ferr(&inv_str, format!(
                "Source file '{}.bu' exists but is not listed in inventory. \
                 Add a line:  {}: fn1, fn2, ...;",
                stem, stem
            )));
        }
    }

    for stem in &inv_stems {
        if !file_stems.contains(stem) {
            errors.push(ferr(&inv_str, format!(
                "Inventory lists '{}' but '{}.bu' does not exist in this folder.",
                stem, stem
            )));
        }
    }

    for entry in &inv.entries {
        if !file_stems.contains(&entry.file) { continue; }

        let bu_path = dir.join(format!("{}.bu", entry.file));
        let source  = match crate::overlay::read_source(&bu_path) {
            Ok(s)  => s,
            Err(_) => continue,
        };

        let sf = match parser::parse_file(&source, false) {
            Ok(BuFile::Source(s)) => s,
            _ => continue,
        };

        let actual_fns: HashSet<&str> = sf.bullets.iter()
            .map(|b| b.name.as_str()).collect();
        let listed_fns: HashSet<&str> = entry.functions.iter()
            .map(|f| f.as_str()).collect();

        for name in &actual_fns {
            if !listed_fns.contains(name) {
                errors.push(ferr(&inv_str, format!(
                    "Function '{}' exists in '{}.bu' but is not listed in inventory.",
                    name, entry.file
                )));
            }
        }

        for name in &listed_fns {
            if !actual_fns.contains(name) {
                errors.push(ferr(&inv_str, format!(
                    "The function '{}' is listed in inventory, but not found in '{}.bu'.",
                    name, entry.file
                )));
            }
        }
    }

    errors
}

// ── Local error constructor ───────────────────────────────────────────────────

fn ferr(file: &str, msg: impl Into<String>) -> ValidationError {
    ValidationError { file: file.to_string(), line: 0, col: 0, message: msg.into() }
}

