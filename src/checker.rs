//! Pre-interpretation checker.
//!
//! Validates that a source file contains no native escape blocks before
//! handing it to the interpreter. Call `check_no_escape` first; if it
//! returns any violations, report them and abort — do not call the
//! interpreter on a file that fails this check.

use crate::ast::{BulletBody, SourceFile};

// ── Types ─────────────────────────────────────────────────────────────────────

/// A bullet that cannot be interpreted because it contains native escape blocks.
pub struct EscapeViolation {
    /// Name of the offending bullet.
    pub bullet:   String,
    /// Backend keywords found inside the bullet (`rust`, `c`, `python`, etc.)
    pub backends: Vec<String>,
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Check a source file for native escape blocks.
///
/// Returns one `EscapeViolation` per bullet that contains escape blocks.
/// An empty `Vec` means the file is safe to pass to the interpreter.
pub fn check_no_escape(source: &SourceFile) -> Vec<EscapeViolation> {
    source.bullets.iter().filter_map(|bullet| {
        if let BulletBody::Natives(blocks) = &bullet.body {
            let backends = blocks.iter()
                .map(|b| b.backend.escape_keyword())
                .collect();
            Some(EscapeViolation {
                bullet:   bullet.name.clone(),
                backends,
            })
        } else {
            None
        }
    }).collect()
}
