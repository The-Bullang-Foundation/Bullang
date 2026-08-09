//! Source file, function, and bullet-level structural validation.

use std::path::Path;
use std::collections::HashSet;
use bullang::ast::*;
use bullang::parser;
use super::{ValidationError, AllErrors};

// ── Source file ───────────────────────────────────────────────────────────────

pub fn validate_source_file(
    path:           &Path,
    folder_rank:    &Rank,
    _inv_map:       &std::collections::HashMap<String, Vec<String>>,
    child_callable: &crate::validator::helpers::Callable,
    folder_lang:    Option<&bullang::ast::Backend>,
) -> AllErrors {
    let mut all = AllErrors::new();

    let source = match crate::overlay::read_source(path) {
        Ok(s)  => s,
        Err(e) => {
            all.push_structural(super::err(path, format!("Could not read file: {}", e)));
            return all;
        }
    };

    let path_str = path.display().to_string();
    let result   = parser::parse_file_tolerant(&source, &path_str);
    all.extend_parse(result.errors);

    let sf = match result.file {
        BuFile::Source(s) => s,
        _                 => return all,
    };

    let is_skirmish = folder_rank == &Rank::Skirmish;

    if sf.bullets.len() > 5 {
        all.push_structural(ferr(&path_str, format!(
            "A source file cannot contain more than 5 functions (found {}).",
            sf.bullets.len()
        )));
    }

    for func in &sf.bullets {
        all.extend_structural(validate_function(func, &path_str, child_callable, is_skirmish));
        // Native block language check
        if let Some(lang) = folder_lang {
            all.extend_structural(validate_native_blocks_lang(func, &path_str, lang));
        } else {
            // No lang declared — native blocks require one
            if let bullang::ast::BulletBody::Natives(blocks) = &func.body {
                if !blocks.is_empty() {
                    all.push_structural(ferr(&path_str, format!(
                        "Function '{}': native block '@{}' requires #lang: to be \
                         declared in this folder's inventory.",
                        func.name, blocks[0].backend.escape_keyword()
                    )));
                }
            }
        }
    }

    all
}

// ── Native block language enforcement ────────────────────────────────────────

/// Every native block in the function must match the folder's declared language.
/// `@c` is accepted in a `#lang: cpp` folder.
fn validate_native_blocks_lang(
    func:    &Bullet,
    path:    &str,
    lang:    &bullang::ast::Backend,
) -> Vec<ValidationError> {
    let blocks = match &func.body {
        bullang::ast::BulletBody::Natives(b) => b,
        _                                   => return vec![],
    };

    let mut errors = Vec::new();
    if blocks.len() > 1 {
        errors.push(ferr(path, format!(
            "Function '{}': only one escape block is allowed per function, found {}. \
             Write one @backend block with the target language code.",
            func.name, blocks.len()
        )));
        return errors;
    }
    for block in blocks {
        let ok = match (&block.backend, lang) {
            // C blocks are valid in C++ folders
            (bullang::ast::Backend::C, bullang::ast::Backend::Cpp) => true,
            (a, b) => a == b,
        };
        if !ok {
            errors.push(ferr(path, format!(
                "Function '{}': '@{}' block is not allowed in a '#lang: {}' folder. \
                 Use '@{}' instead, or move this function to a folder with '#lang: {}'.",
                func.name,
                block.backend.escape_keyword(),
                lang_ext(lang),
                lang.escape_keyword(),
                lang_ext(&block.backend),
            )));
        }
    }
    errors
}

// ── Function ──────────────────────────────────────────────────────────────────

pub fn validate_function(
    func:        &Bullet,
    path:        &str,
    callable:    &crate::validator::helpers::Callable,
    is_skirmish: bool,
) -> Vec<ValidationError> {
    match &func.body {
        BulletBody::Natives(blocks) => {
            match blocks.iter().find(|b| matches!(b.backend, bullang::ast::Backend::Unknown(_))) {
                Some(b) => {
                    if let bullang::ast::Backend::Unknown(kw) = &b.backend {
                        vec![ferr(path, format!(
                            "Function '{}': '@{}' is not a supported backend. \
                             Supported escape blocks: @rust, @python, @c, @cpp, @go.",
                            func.name, kw
                        ))]
                    } else { vec![] }
                }
                None => vec![],
            }
        }
        BulletBody::Builtin(name) => {
            if !crate::stdlib::is_known_builtin(name) {
                vec![ferr(path, format!(
                    "Function '{}': 'builtin::{}' is not a known builtin. \
                     Run `bullang stdlib --list` to see available builtins.",
                    func.name, name
                ))]
            } else {
                vec![]
            }
        }
        BulletBody::Pipes(pipes) => validate_bullets(
            pipes, &func.name, func.output.as_ref().map(|o| o.name.as_str()),
            &func.params, path, callable, is_skirmish,
        ),
    }
}

// ── Bullets ───────────────────────────────────────────────────────────────────

pub fn validate_bullets(
    bullets:     &[Pipe],
    func_name:   &str,
    output_name: Option<&str>,
    params:      &[Param],
    path:        &str,
    callable:    &crate::validator::helpers::Callable,
    is_skirmish: bool,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if bullets.len() > 5 {
        errors.push(ferr(path, format!(
            "Function '{}': cannot contain more than 5 bullets (found {}).",
            func_name, bullets.len()
        )));
    }

    let param_names: HashSet<&str> = params.iter().map(|p| p.name.as_str()).collect();
    let mut bound:    HashSet<String> = HashSet::new();
    let mut consumed: HashSet<String> = HashSet::new();
    let last = bullets.len().saturating_sub(1);

    for (i, bullet) in bullets.iter().enumerate() {
        // Both halves of the bullet name values. Only `inputs` was walked, so
        // an undefined name in the *expression* — `(a) : a + typo -> {r};` —
        // went unreported, and a binding used only in an expression was
        // wrongly called unused.
        let mut named: Vec<String> = Vec::new();
        for input in &bullet.inputs {
            collect_idents(input, &mut named);
        }
        // Whether the expression names a *value* depends on what it is, which
        // is the classifier's question. `(s) : loud -> {r};` has `loud` as a
        // callee — checked against the inventory by `collect_call_errors` —
        // not as a binding that must already exist. Only a self-contained
        // expression names values here.
        if let crate::pipe::PipeRhs::Expr(expr) = crate::pipe::classify(bullet) {
            collect_idents(expr, &mut named);
        }

        for name in named {
            if param_names.contains(name.as_str()) || bound.contains(name.as_str()) {
                consumed.insert(name);
            } else {
                errors.push(serr(path, bullet.span, format!(
                    "Function '{}' bullet {}: '{}' is not a parameter or an \
                     earlier binding.",
                    func_name, i + 1, name
                )));
            }
        }

        // `(a) : helper -> {r};` names its callee as a bare identifier, not an
        // `Atom::Call`, so walking the expression alone never saw it — the
        // inventory and cross-region checks silently skipped the commonest
        // way to write a call.
        if let crate::pipe::PipeRhs::Call { name, .. } = crate::pipe::classify(bullet) {
            check_callee(name, func_name, path, bullet.span, callable, is_skirmish, &mut errors);
        }
        collect_call_errors(
            &bullet.expr, func_name, path, bullet.span,
            callable, is_skirmish, &mut errors,
        );

        if bullet.binding.as_ref().map(|b| bound.contains(b)).unwrap_or(false) {
            errors.push(serr(path, bullet.span, format!(
                "Function '{}': '{{{}}}' is assigned more than once.",
                func_name, bullet.binding.as_deref().unwrap_or("_")
            )));
        }

        // Four cases, not one comparison. The old code flattened "this
        // function declares no output" to `""` on one side and left it as
        // `None` on the other, so `None != Some("")` — and *every* function
        // without a return value failed validation, always.
        if i == last {
            match (output_name, bullet.binding.as_deref()) {
                // Binds the declared output. Correct.
                (Some(want), Some(got)) if got == want => {}
                (Some(want), Some(got)) => errors.push(serr(path, bullet.span, format!(
                    "Function '{}': last bullet binds '{{{}}}' but the declared output \
                     is '{{{}}}'.",
                    func_name, got, want
                ))),
                (Some(want), None) => errors.push(serr(path, bullet.span, format!(
                    "Function '{}': last bullet discards its result, but the function \
                     declares an output '{{{}}}'.",
                    func_name, want
                ))),
                // Declares nothing, produces nothing. Correct.
                (None, None) => {}
                (None, Some(got)) => errors.push(serr(path, bullet.span, format!(
                    "Function '{}': last bullet binds '{{{}}}' but the function declares \
                     no output. Discard it with '-> {{}}' or declare a return value.",
                    func_name, got
                ))),
            }
        }

        if let Some(ref b) = bullet.binding {
            bound.insert(b.clone());
        }
    }

    // `bound` is a HashSet, so iterating it directly made the order of these
    // errors differ between runs on the same input.
    let mut unused: Vec<&String> = bound.iter().collect();
    unused.sort();
    for b in unused {
        if Some(b.as_str()) != output_name && !consumed.contains(b) {
            errors.push(ferr(path, format!(
                "Function '{}': '{{{}}}' is produced but never used.",
                func_name, b
            )));
        }
    }

    errors
}

// ── Call / atom traversal ─────────────────────────────────────────────────────

pub fn collect_call_errors(
    expr:        &Expr,
    func_name:   &str,
    path:        &str,
    span:        Span,
    callable:    &crate::validator::helpers::Callable,
    is_skirmish: bool,
    errors:      &mut Vec<ValidationError>,
) {
    match expr {
        Expr::Atom(a)      => check_atom(a, func_name, path, span, callable, is_skirmish, errors),
        Expr::BinOp(b)     => {
            check_atom(&b.lhs, func_name, path, span, callable, is_skirmish, errors);
            check_atom(&b.rhs, func_name, path, span, callable, is_skirmish, errors);
        }
        Expr::Tuple(exprs) => {
            for e in exprs {
                collect_call_errors(e, func_name, path, span, callable, is_skirmish, errors);
            }
        }
    }
}

pub fn check_atom(
    atom:        &Atom,
    func_name:   &str,
    path:        &str,
    span:        Span,
    callable:    &crate::validator::helpers::Callable,
    is_skirmish: bool,
    errors:      &mut Vec<ValidationError>,
) {
    if let Atom::Call { name, .. } = atom {
        check_callee(name, func_name, path, span, callable, is_skirmish, errors);
    }
}

/// The rules that apply to calling `name` from `func_name`, whichever syntax
/// was used to write the call.
pub fn check_callee(
    name:        &str,
    func_name:   &str,
    path:        &str,
    span:        Span,
    callable:    &crate::validator::helpers::Callable,
    is_skirmish: bool,
    errors:      &mut Vec<ValidationError>,
) {
    {
        if is_skirmish {
            errors.push(serr(path, span, format!(
                "Function '{}': skirmish files cannot call other functions (found call to '{}').",
                func_name, name
            )));
            return;
        }
        // A call into another language region is an error in its own right,
        // and saying so is the whole point — reporting the function as
        // "not listed in any child inventory" would send the author looking
        // for a missing declaration that is in fact right where they put it.
        if let Some(region) = callable.other_region.get(name) {
            errors.push(serr(path, span, format!(
                "Function '{}': calls '{}', which belongs to the language region \
                 rooted at '{}'. Each region is transpiled to its own language in \
                 its own directory, and Bullang generates no FFI between them — so \
                 a call cannot cross that boundary. Move one of the two, or remove \
                 that folder's '#lang' so both are in one region.",
                func_name, name, region.display()
            )));
            return;
        }
        if !callable.is_empty() && !callable.contains(name) {
            errors.push(serr(path, span, format!(
                "Function '{}': calls '{}' which is not listed in any child inventory.",
                func_name, name
            )));
        }
    }
}

// ── Local error constructors ──────────────────────────────────────────────────

fn serr(file: &str, span: Span, msg: impl Into<String>) -> ValidationError {
    ValidationError { file: file.to_string(), line: span.line, col: span.col, message: msg.into() }
}

fn ferr(file: &str, msg: impl Into<String>) -> ValidationError {
    ValidationError { file: file.to_string(), line: 0, col: 0, message: msg.into() }
}

/// The `#lang:` spelling of a backend. An unrecognised backend has no
/// extension, so it is named as the author wrote it.
fn lang_ext(b: &bullang::ast::Backend) -> String {
    b.ext().map(|e| e.to_string()).unwrap_or_else(|| b.escape_keyword())
}

/// Every identifier named in `expr`, in source order.
///
/// A name in an expression is a use exactly as a name in the input list is —
/// the scope checker only ever looked at the latter.
fn collect_idents(expr: &bullang::ast::Expr, out: &mut Vec<String>) {
    use bullang::ast::{Atom, CallArg, Expr};
    match expr {
        Expr::Atom(a)  => collect_idents_atom(a, out),
        Expr::BinOp(b) => {
            collect_idents_atom(&b.lhs, out);
            collect_idents_atom(&b.rhs, out);
        }
        Expr::Tuple(items) => {
            for item in items {
                collect_idents(item, out);
            }
        }
    }

    fn collect_idents_atom(atom: &Atom, out: &mut Vec<String>) {
        match atom {
            Atom::Ident(name) => out.push(name.clone()),
            // A call's *name* is a function, checked separately against the
            // inventory; its arguments are values.
            Atom::Call { args, .. } => {
                for arg in args {
                    let CallArg::Value(v) = arg;
                    out.push(v.clone());
                }
            }
            Atom::FieldAccess { base, .. } => out.push(base.clone()),
            Atom::Index { base, idx }      => {
                out.push(base.clone());
                collect_idents(idx, out);
            }
            Atom::Slice { base, from, to } => {
                out.push(base.clone());
                collect_idents(from, out);
                collect_idents(to, out);
            }
            Atom::Unary { rhs, .. }        => collect_idents_atom(rhs, out),
            Atom::BuiltinExpr { args, .. } => {
                for arg in args {
                    collect_idents(arg, out);
                }
            }
            // A bare builtin names no values of its own — its arguments are
            // the bullet's inputs, already walked.
            Atom::BuiltinNoArgs(_)
            | Atom::Integer(_)
            | Atom::Float(_)
            | Atom::StringLit(_)
            // A template names bindings inside `{}`; reading them out needs
            // the template parsed, which is the same gap sanitize.rs has.
            | Atom::Interp(_)
            | Atom::EnumVariant { .. } => {}
        }
    }
}
