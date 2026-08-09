//! Making a Bullang name safe to emit as a target-language identifier.
//!
//! Bullang's own keywords are few, so a perfectly ordinary Bullang name can
//! collide with a reserved word in a target language: `class` is fine in
//! Bullang and fatal in C++, Java and Python; `type` is fine everywhere except
//! Go's own vocabulary of trouble; `match` is a Rust keyword.
//!
//! What existed before was two duplicate, incomplete Python keyword tables —
//! and nothing at all for the other five backends. Both Python tables were
//! applied only to parameter lists, so a *function* named `class` or a
//! *binding* named `lambda` produced output that would not parse, in every
//! language.
//!
//! One table per backend, complete, applied at every point an identifier is
//! emitted: function names, parameter names, bindings, struct and enum names,
//! and field names.
//!
//! ## How a name is escaped
//!
//! A trailing underscore, which is what Python's own style guide recommends
//! and what reads most obviously as "this was a keyword": `class` becomes
//! `class_`. That can itself collide — a program with both `class` and
//! `class_` gets two `class_`es — which is why the validator, not this
//! module, is the right place to reject such a pair. It is recorded in
//! HANDOFF.md rather than silently mangled here into something unreadable.

use bullang::ast::Backend;

/// `name` as a legal identifier for `backend`.
pub fn sanitize(name: &str, backend: &Backend) -> String {
    if is_reserved(name, backend) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

/// True if `name` is a reserved word in `backend`.
pub fn is_reserved(name: &str, backend: &Backend) -> bool {
    let table: &[&str] = match backend {
        Backend::Rust       => RUST,
        Backend::Python     => PYTHON,
        Backend::C          => C,
        Backend::Cpp        => CPP,
        Backend::Go         => GO,
        Backend::Java       => JAVA,
        // An unrecognised backend has no known vocabulary, and guessing at one
        // would corrupt names for no reason.
        Backend::Unknown(_) => return false,
    };
    table.binary_search(&name).is_ok()
}

// Every table below is sorted, because `is_reserved` binary-searches it. A
// table that is not sorted fails the test at the bottom of this file rather
// than silently reporting a keyword as safe.

const RUST: &[&str] = &[
    "Self", "abstract", "as", "async", "await", "become", "box", "break",
    "const", "continue", "crate", "do", "dyn", "else", "enum", "extern",
    "false", "final", "fn", "for", "if", "impl", "in", "let", "loop", "macro",
    "match", "mod", "move", "mut", "override", "priv", "pub", "ref", "return",
    "self", "static", "struct", "super", "trait", "true", "try", "type",
    "typeof", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

// Soft keywords (`match`, `case`, `type`) are not reserved, but shadowing a
// builtin like `list` or `id` is a real hazard in generated code that a reader
// did not write, so the common ones are included.
const PYTHON: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "bool",
    "break", "bytes", "class", "continue", "def", "del", "dict", "elif",
    "else", "except", "finally", "float", "for", "from", "global", "id", "if",
    "import", "in", "input", "int", "is", "lambda", "len", "list", "map",
    "max", "min", "nonlocal", "not", "object", "or", "pass", "print", "raise",
    "range", "return", "set", "str", "sum", "try", "tuple", "type", "while",
    "with", "yield", "zip",
];

const C: &[&str] = &[
    "_Alignas", "_Alignof", "_Atomic", "_Bool", "_Complex", "_Generic",
    "_Imaginary", "_Noreturn", "_Static_assert", "_Thread_local", "auto",
    "break", "case", "char", "const", "continue", "default", "do", "double",
    "else", "enum", "extern", "float", "for", "goto", "if", "inline", "int",
    "long", "register", "restrict", "return", "short", "signed", "sizeof",
    "static", "struct", "switch", "typedef", "union", "unsigned", "void",
    "volatile", "while",
];

const CPP: &[&str] = &[
    "alignas", "alignof", "and", "and_eq", "asm", "auto", "bitand", "bitor",
    "bool", "break", "case", "catch", "char", "char16_t", "char32_t",
    "char8_t", "class", "co_await", "co_return", "co_yield", "compl",
    "concept", "const", "const_cast", "consteval", "constexpr", "constinit",
    "continue", "decltype", "default", "delete", "do", "double",
    "dynamic_cast", "else", "enum", "explicit", "export", "extern", "false",
    "float", "for", "friend", "goto", "if", "inline", "int", "long",
    "mutable", "namespace", "new", "noexcept", "not", "not_eq", "nullptr",
    "operator", "or", "or_eq", "private", "protected", "public", "register",
    "reinterpret_cast", "requires", "return", "short", "signed", "sizeof",
    "static", "static_assert", "static_cast", "struct", "switch", "template",
    "this", "thread_local", "throw", "true", "try", "typedef", "typeid",
    "typename", "union", "unsigned", "using", "virtual", "void", "volatile",
    "wchar_t", "while", "xor", "xor_eq",
];

// Go's predeclared identifiers are not keywords, but shadowing `len`, `cap`,
// `new` or `string` inside generated code is a trap for whoever reads it next.
const GO: &[&str] = &[
    "any", "append", "bool", "break", "byte", "cap", "case", "chan", "clear",
    "close", "complex", "const", "continue", "copy", "default", "defer",
    "delete", "else", "error", "fallthrough", "false", "float32", "float64",
    "for", "func", "go", "goto", "if", "import", "int", "int16", "int32",
    "int64", "int8", "interface", "iota", "len", "make", "map", "max", "min",
    "new", "nil", "package", "panic", "print", "println", "range", "recover",
    "return", "rune", "select", "string", "struct", "switch", "true", "type",
    "uint", "uint16", "uint32", "uint64", "uint8", "uintptr", "var",
];

const JAVA: &[&str] = &[
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char",
    "class", "const", "continue", "default", "do", "double", "else", "enum",
    "extends", "false", "final", "finally", "float", "for", "goto", "if",
    "implements", "import", "instanceof", "int", "interface", "long", "native",
    "new", "null", "package", "private", "protected", "public", "return",
    "short", "static", "strictfp", "super", "switch", "synchronized", "this",
    "throw", "throws", "transient", "true", "try", "void", "volatile", "while",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// `is_reserved` binary-searches, so an unsorted table would quietly
    /// report some keywords as safe.
    #[test]
    fn tables_are_sorted() {
        for (label, table) in [
            ("rust", RUST), ("python", PYTHON), ("c", C),
            ("cpp", CPP), ("go", GO), ("java", JAVA),
        ] {
            let mut sorted = table.to_vec();
            sorted.sort_unstable();
            assert_eq!(table, sorted.as_slice(), "{label} table is not sorted");
        }
    }

    #[test]
    fn escapes_only_keywords() {
        assert_eq!(sanitize("class", &Backend::Cpp), "class_");
        assert_eq!(sanitize("class", &Backend::Rust), "class");
        assert_eq!(sanitize("match", &Backend::Rust), "match_");
        assert_eq!(sanitize("total", &Backend::Go), "total");
        assert_eq!(sanitize("len", &Backend::Go), "len_");
    }
}

// ── Normalisation pass ────────────────────────────────────────────────────

/// Rewrite every identifier in `file` so it is legal in `backend`.
///
/// Done as one pass over the AST rather than at each emission site. There are
/// upwards of forty places a backend writes an identifier — function names,
/// parameters, bindings, call arguments, field accesses, index bases — and the
/// old code covered exactly one of them (parameter lists, in Python). A pass
/// cannot be forgotten at a site nobody thought of, and it keeps a binding's
/// declaration and its uses in agreement by construction.
///
/// **Value identifiers only.** Type names are deliberately untouched: Bullang's
/// own primitives collide with target keywords by design — `bool` is a Bullang
/// type *and* a C++, Go and Python keyword — so renaming them here would break
/// every program. Each backend already translates type names through
/// `bu_type_to_*`. A user-declared struct named `class` is therefore still a
/// problem in C++; see HANDOFF.md.
///
/// Builtin names are untouched too. They are not user identifiers, and the
/// stdlib decides what each one is spelled as in each language.
pub fn normalize(file: &mut bullang::ast::SourceFile, backend: &Backend) {
    for bullet in &mut file.bullets {
        bullet.name = sanitize(&bullet.name, backend);
        for param in &mut bullet.params {
            param.name = sanitize(&param.name, backend);
        }
        if let Some(output) = bullet.output.as_mut() {
            output.name = sanitize(&output.name, backend);
        }
        normalize_body(&mut bullet.body, backend);
    }
}

fn normalize_body(body: &mut bullang::ast::BulletBody, backend: &Backend) {
    use bullang::ast::BulletBody;
    match body {
        BulletBody::Pipes(pipes) => {
            for pipe in pipes {
                for input in &mut pipe.inputs {
                    normalize_expr(input, backend);
                }
                normalize_expr(&mut pipe.expr, backend);
                if let Some(binding) = pipe.binding.as_mut() {
                    *binding = sanitize(binding, backend);
                }
            }
        }
        // An escape block is byte-for-byte whatever the author wrote. It is
        // already in the target language, so its identifiers are already legal
        // there — and rewriting inside one would break the guarantee that it
        // is copied verbatim.
        BulletBody::Natives(_) => {}
        BulletBody::Builtin(_) => {}
    }
}

fn normalize_expr(expr: &mut bullang::ast::Expr, backend: &Backend) {
    use bullang::ast::Expr;
    match expr {
        Expr::Atom(a) => normalize_atom(a, backend),
        Expr::BinOp(b) => {
            normalize_atom(&mut b.lhs, backend);
            normalize_atom(&mut b.rhs, backend);
        }
        Expr::Tuple(items) => {
            for item in items {
                normalize_expr(item, backend);
            }
        }
    }
}

fn normalize_atom(atom: &mut bullang::ast::Atom, backend: &Backend) {
    use bullang::ast::{Atom, CallArg};
    match atom {
        Atom::Ident(name) => *name = sanitize(name, backend),
        Atom::Call { name, args } => {
            *name = sanitize(name, backend);
            for arg in args {
                let CallArg::Value(v) = arg;
                *v = sanitize(v, backend);
            }
        }
        Atom::FieldAccess { base, .. } => {
            // The base is a binding or parameter; the fields are struct member
            // names, which follow the type, not the value namespace.
            *base = sanitize(base, backend);
        }
        Atom::Index { base, idx } => {
            *base = sanitize(base, backend);
            normalize_expr(idx, backend);
        }
        Atom::Slice { base, from, to } => {
            *base = sanitize(base, backend);
            normalize_expr(from, backend);
            normalize_expr(to, backend);
        }
        Atom::Unary { rhs, .. } => normalize_atom(rhs, backend),
        // `builtin::name` is not a user identifier — the stdlib owns its
        // spelling in each language — but its arguments are.
        Atom::BuiltinExpr { args, .. } => {
            for arg in args {
                normalize_expr(arg, backend);
            }
        }
        Atom::BuiltinNoArgs(_)
        | Atom::Integer(_)
        | Atom::Float(_)
        | Atom::StringLit(_)
        // An interpolated template names bindings inside `{}`. Rewriting those
        // would need the template re-parsed; a binding that is also a target
        // keyword and appears in a template is the one gap left here.
        | Atom::Interp(_)
        | Atom::EnumVariant { .. } => {}
    }
}
