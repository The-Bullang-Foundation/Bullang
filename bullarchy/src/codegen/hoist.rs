//! What a file's builtins need declared above them, for any backend.
//!
//! Every backend used to collect this for itself. Rust and C had one
//! collector each, C++ borrowed C's, Go had a hand-written `needs_tern_helper`
//! that knew about exactly one builtin, and Java and Python had no collection
//! at all — their imports were a fixed list written into the preamble, so a
//! builtin needing anything else silently produced code that would not
//! compile.
//!
//! They also all walked the same wrong shape: only `pipe.expr` at the top
//! level. An inline `builtin::to_upper(s)` nested inside a call argument, a
//! tuple, or either side of a binary operation contributed nothing, so its
//! imports and helpers were missing from the file that used it.
//!
//! This module walks the whole expression tree once, asks each builtin what
//! it needs through `stdlib::requirements`, and hands back a deduplicated,
//! stably ordered result. Backends differ in *where* these go — Go's inside
//! `import ( ... )`, C's as `#include` lines, Java's as members of the
//! generated class — so placement stays with the backend and only the
//! gathering is shared.

use bullang::ast::*;
use crate::stdlib;
use std::collections::BTreeSet;

/// Everything a file's builtins need at file scope, deduplicated.
pub struct Hoisted {
    pub imports: Vec<&'static str>,
    pub helpers: Vec<&'static str>,
    /// Imports the helpers need, kept apart from the call-site ones. Only Go
    /// distinguishes them — see `Requirements::helper_imports`.
    pub helper_imports: Vec<&'static str>,
}

/// Every builtin named anywhere in `file`, in any position.
pub fn builtin_names(file: &SourceFile) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for func in &file.bullets {
        walk_body(&func.body, &mut out);
    }
    out
}

/// The imports and helpers `file` needs for `backend`.
pub fn requirements(file: &SourceFile, backend: &Backend) -> Hoisted {
    let mut imports: BTreeSet<&'static str> = BTreeSet::new();
    let mut helpers: BTreeSet<&'static str> = BTreeSet::new();
    let mut helper_imports: BTreeSet<&'static str> = BTreeSet::new();

    for name in builtin_names(file) {
        let req = stdlib::requirements(&name, backend);
        imports.extend(req.imports);
        helpers.extend(req.helpers);
        helper_imports.extend(req.helper_imports);
    }

    // Only Go emits helpers away from their callers. Everywhere else the
    // helper sits in the same file, so its imports are simply imports.
    if !matches!(backend, Backend::Go) {
        imports.extend(helper_imports.iter().copied());
        helper_imports.clear();
    }

    let mut helpers: Vec<&'static str> = helpers.into_iter().collect();
    if matches!(backend, Backend::C | Backend::Cpp) {
        // C and C++ need declaration before use, and the deduplicating set
        // knows nothing about which helper calls which.
        helpers.sort_by_key(|h| {
            crate::stdlib::helper_rank(h, backend).unwrap_or(usize::MAX)
        });
    }

    Hoisted {
        imports: imports.into_iter().collect(),
        helpers,
        helper_imports: helper_imports.into_iter().collect(),
    }
}

// ── The walk ──────────────────────────────────────────────────────────────

fn walk_body(body: &BulletBody, out: &mut BTreeSet<String>) {
    match body {
        BulletBody::Pipes(pipes) => {
            for pipe in pipes {
                for input in &pipe.inputs {
                    walk_expr(input, out);
                }
                walk_expr(&pipe.expr, out);
            }
        }
        // `let f() { -> builtin::close; }` — the whole body is one builtin,
        // with no pipe around it.
        BulletBody::Builtin(name) => {
            out.insert(name.clone());
        }
        // An escape block is opaque by design: byte-for-byte, whatever the
        // author wrote. Bullang does not read inside it, so it contributes
        // no builtins — anything it needs, it declares itself.
        BulletBody::Natives(_) => {}
    }
}

fn walk_expr(expr: &Expr, out: &mut BTreeSet<String>) {
    match expr {
        Expr::Atom(a) => walk_atom(a, out),
        Expr::BinOp(b) => {
            walk_atom(&b.lhs, out);
            walk_atom(&b.rhs, out);
        }
        Expr::Tuple(items) => {
            for item in items {
                walk_expr(item, out);
            }
        }
    }
}

fn walk_atom(atom: &Atom, out: &mut BTreeSet<String>) {
    match atom {
        Atom::BuiltinNoArgs(name) => {
            out.insert(name.clone());
        }
        Atom::BuiltinExpr { name, args } => {
            out.insert(name.clone());
            for arg in args {
                walk_expr(arg, out);
            }
        }
        Atom::Unary { rhs, .. } => walk_atom(rhs, out),
        Atom::Index { idx, .. } => walk_expr(idx, out),
        Atom::Slice { from, to, .. } => {
            walk_expr(from, out);
            walk_expr(to, out);
        }
        // A call's arguments are `CallArg::Value(String)` — plain names, with
        // no expression to descend into.
        Atom::Call { .. }
        | Atom::Ident(_)
        | Atom::Integer(_)
        | Atom::Float(_)
        | Atom::StringLit(_)
        | Atom::Interp(_)
        | Atom::FieldAccess { .. }
        | Atom::EnumVariant { .. } => {}
    }
}
