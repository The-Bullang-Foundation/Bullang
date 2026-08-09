//! Code generation — AST → Rust source.

use bullang::ast::*;
use crate::stdlib;

// ── Hoisted imports ───────────────────────────────────────────────────────────

fn emit_imports(out: &mut String, file: &SourceFile) {
    let hoisted = crate::codegen::hoist::requirements(file, &Backend::Rust);
    for import in &hoisted.imports {
        out.push_str(import);
        out.push('\n');
    }
    if !hoisted.imports.is_empty() {
        out.push('\n');
    }
    for helper in &hoisted.helpers {
        out.push_str(helper);
        out.push('\n');
    }
}

// ── Source file → Rust ────────────────────────────────────────────────────────

pub fn emit_source(file: &SourceFile) -> String {
    let mut out = String::new();
    out.push_str("#[allow(unused_imports)]\n");
    out.push_str("use crate::*;\n");
    emit_imports(&mut out, file);
    for func in &file.bullets {
        out.push_str(&emit_function(func, &Backend::Rust));
        out.push('\n');
    }
    out
}

/// Bare single-file mode: only the function bodies (plus any hoisted
/// builtin imports), no `use crate::*`, no attributes, no other preamble.
pub fn emit_bare_rs(file: &SourceFile) -> String {
    let mut out = String::new();
    emit_imports(&mut out, file);
    for func in &file.bullets {
        out.push_str(&emit_function(func, &Backend::Rust));
        out.push('\n');
    }
    out
}

// ── Struct emitter ────────────────────────────────────────────────────────────

pub fn emit_struct_rs(s: &bullang::ast::StructDef) -> String {
    let mut out = String::new();
    // PartialEq and Eq to match the enum emitter — without them a struct
    // could not be compared, including by the `==` a bullet is allowed to
    // write.
    out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
    out.push_str(&format!("pub struct {} {{\n", s.name));
    for field in &s.fields {
        out.push_str(&format!("    pub {}: {},\n", field.name, bu_type_to_rust(&field.ty)));
    }
    out.push_str("}\n");
    out
}

pub fn emit_enum_rs(e: &bullang::ast::EnumDef) -> String {
    let mut out = String::new();
    out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
    out.push_str(&format!("pub enum {} {{\n", e.name));
    for v in &e.variants {
        out.push_str(&format!("    {},\n", v.name));
    }
    out.push_str("}\n");
    out
}

// ── main.bu → main.rs ─────────────────────────────────────────────────────────

/// Emits src/main.rs from the parsed main.bu.
/// The main function gets `fn main()` — no pub, no return type.
/// All other functions in main.bu (helpers) get `fn` but not `pub`.
pub fn emit_main(file: &SourceFile, crate_name: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("use {}::*;\n", crate_name));
    emit_imports(&mut out, file);
    for func in &file.bullets {
        if func.name == "main" {
            out.push_str(&emit_main_function(func));
        } else {
            out.push_str(&emit_function(func, &Backend::Rust));
        }
        out.push('\n');
    }
    out
}

/// Emits Cargo.toml with both a [[bin]] and [lib] section when main.rs exists.
pub fn emit_cargo_toml_with_main(crate_name: &str) -> String {
    format!(
        "[package]\n\
         name    = \"{name}\"\n\
         version = \"0.1.0\"\n\
         edition = \"2021\"\n\n\
         [[bin]]\n\
         name = \"{name}\"\n\
         path = \"src/main.rs\"\n\n\
         [lib]\n\
         name = \"{name}\"\n\
         path = \"src/lib.rs\"\n\n\
         [dependencies]\n",
        name = crate_name
    )
}

/// Emits Cargo.toml as a library-only crate (no main.bu present).
pub fn emit_cargo_toml(crate_name: &str) -> String {
    format!(
        "[package]\nname    = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
        crate_name
    )
}

// ── Module files ──────────────────────────────────────────────────────────────

pub fn emit_mod_rs(child_modules: &[String]) -> String {
    let mut out = String::new();
    for module in child_modules {
        out.push_str(&format!("pub mod {};\n", module));
    }
    if !child_modules.is_empty() {
        out.push('\n');
        for module in child_modules {
            out.push_str(&format!("pub use {}::*;\n", module));
        }
    }
    out
}

pub fn emit_lib_rs(child_modules: &[String], structs: &[bullang::ast::StructDef], enums: &[bullang::ast::EnumDef]) -> String {
    let mut out = String::new();
    out.push_str("#![allow(unused_imports)]\n\n");
    for s in structs {
        out.push_str(&emit_struct_rs(s));
        out.push('\n');
    }
    for e in enums {
        out.push_str(&emit_enum_rs(e));
        out.push('\n');
    }
    for module in child_modules {
        out.push_str(&format!("pub mod {};\n", module));
    }
    if !child_modules.is_empty() {
        out.push('\n');
        for module in child_modules {
            out.push_str(&format!("pub use {}::*;\n", module));
        }
    }
    out
}

// ── Type translation: Bullang syntax → Rust syntax ──────────────────────────
//
// Bullang uses bracket generics (Vec[T], Option[T]) and Tuple[T,U] / Fn[T->U].
// Rust requires angle-bracket generics (Vec<T>, Option<T>), tuple syntax (T, U),
// and fn pointer syntax fn(T)->U.  This function translates at codegen time so
// developers write clean Bullang syntax while the output is valid Rust.

pub fn bu_type_to_rust(ty: &BuType) -> String {
    match ty {
        BuType::Named(s)     => translate_named_to_rust(s),
        BuType::Tuple(inner) => format!(
            "({})",
            inner.iter().map(bu_type_to_rust).collect::<Vec<_>>().join(", ")
        ),
        BuType::Unknown      => "_".to_string(),
    }
}

fn translate_named_to_rust(s: &str) -> String {
    // Vec[T] → Vec<T>
    if s.starts_with("Vec[") && s.ends_with(']') {
        let inner = &s[4..s.len()-1];
        return format!("Vec<{}>", translate_named_to_rust(inner));
    }
    // Option[T] → Option<T>
    if s.starts_with("Option[") && s.ends_with(']') {
        let inner = &s[7..s.len()-1];
        return format!("Option<{}>", translate_named_to_rust(inner));
    }
    // Tuple[T, U] → (T, U)
    if s.starts_with("Tuple[") && s.ends_with(']') {
        let inner = &s[6..s.len()-1];
        let parts = split_top_level(inner, ',');
        let translated: Vec<String> = parts.iter()
            .map(|p| translate_named_to_rust(p.trim()))
            .collect();
        return format!("({})", translated.join(", "));
    }
    // Fn[T, U -> V] → fn(T, U) -> V
    if s.starts_with("Fn[") && s.ends_with(']') {
        let inner = &s[3..s.len()-1];
        return translate_fn_to_rust(inner);
    }
    // Box[T] → Box<T>
    if s.starts_with("Box[") && s.ends_with(']') {
        let inner = &s[4..s.len()-1];
        return format!("Box<{}>", translate_named_to_rust(inner));
    }
    // HashMap[K, V] → HashMap<K, V>
    if s.starts_with("HashMap[") && s.ends_with(']') {
        let inner = &s[8..s.len()-1];
        let parts = split_top_level(inner, ',');
        if parts.len() == 2 {
            return format!("HashMap<{}, {}>",
                translate_named_to_rust(parts[0].trim()),
                translate_named_to_rust(parts[1].trim()));
        }
    }
    // Already valid Rust (angle brackets, primitives, &T etc.) — pass through
    s.to_string()
}

fn translate_fn_to_rust(inner: &str) -> String {
    // inner is contents of Fn[...]: "T, U -> V" or "T -> V" or "-> V" or ""
    if let Some(arrow) = inner.find("->") {
        let params_str = inner[..arrow].trim();
        let ret_str    = inner[arrow+2..].trim();
        let params: Vec<String> = if params_str.is_empty() { vec![] }
            else {
                split_top_level(params_str, ',').iter()
                    .map(|p| translate_named_to_rust(p.trim()))
                    .collect()
            };
        let ret = if ret_str.is_empty() { String::new() }
            else { translate_named_to_rust(ret_str) };
        if ret.is_empty() { format!("fn({})", params.join(", ")) }
        else              { format!("fn({}) -> {}", params.join(", "), ret) }
    } else if inner.trim().is_empty() {
        "fn()".to_string()
    } else {
        // No arrow: treat as return type with no params
        format!("fn() -> {}", translate_named_to_rust(inner.trim()))
    }
}

/// Split a string on `sep` while respecting nested bracket depth.
fn split_top_level(s: &str, sep: char) -> Vec<String> {
    let mut parts  = Vec::new();
    let mut current = String::new();
    let mut depth   = 0i32;
    for ch in s.chars() {
        match ch {
            '[' | '(' | '<' => { depth += 1; current.push(ch); }
            ']' | ')' | '>' => { depth -= 1; current.push(ch); }
            c if c == sep && depth == 0 => {
                parts.push(current.trim().to_string());
                current = String::new();
            }
            _ => { current.push(ch); }
        }
    }
    if !current.trim().is_empty() { parts.push(current.trim().to_string()); }
    parts
}

// ── Function emitters ─────────────────────────────────────────────────────────

/// Emit a regular function. All are `pub` since there is no private code in Bullang.
fn emit_function(func: &Bullet, backend: &Backend) -> String {
    let mut out = String::new();

    let params = func.params.iter()
        .map(|p| format!("{}: {}", p.name, bu_type_to_rust(&p.ty)))
        .collect::<Vec<_>>().join(", ");
    let ret_ty = func.output.as_ref()
        .map(|o| bu_type_to_rust(&o.ty))
        .unwrap_or_else(|| "()".to_string());

    if func.type_params.is_empty() {
        out.push_str(&format!("pub fn {}({}) -> {} {{\n", func.name, params, ret_ty));
    } else {
        // Infer bounds: PartialOrd if body uses comparison ops, Clone if it uses clone-requiring ops.
        let bounds = rust_generic_bounds(func);
        let type_str = func.type_params.iter()
            .map(|t| format!("{}: {}", t, bounds))
            .collect::<Vec<_>>().join(", ");
        out.push_str(&format!("pub fn {}<{}>({}) -> {} {{\n", func.name, type_str, params, ret_ty));
    }

    let returns_unit = super::codegen_c::output_is_unit(func);
    emit_body(&mut out, &func.body, &func.params, backend, returns_unit);
    out.push_str("}\n");
    out
}

/// The trait bounds a generic Rust function actually needs.
///
/// This used to add `Clone` to every type parameter "for safety" and
/// `PartialOrd` whenever any comparison appeared. Both were guesses: `Clone`
/// on a value the function only passes along is a bound the caller has to
/// satisfy for no reason, and `PartialOrd` was demanded even when the only
/// comparison was `==`, which needs `PartialEq`. Bounds now follow the
/// operators the body actually uses.
fn rust_generic_bounds(func: &Bullet) -> String {
    let mut ordering = false;
    let mut equality = false;

    if let BulletBody::Pipes(pipes) = &func.body {
        for pipe in pipes {
            if let Expr::BinOp(b) = &pipe.expr {
                match b.op.as_str() {
                    "<" | ">" | "<=" | ">=" => ordering = true,
                    "==" | "!="             => equality = true,
                    _ => {}
                }
            }
        }
    }

    // PartialOrd implies PartialEq, so naming both would be redundant.
    if ordering {
        "PartialOrd".to_string()
    } else if equality {
        "PartialEq".to_string()
    } else {
        // No constraint at all is right for a value the function only moves.
        "Sized".to_string()
    }
}

/// Emit the `main` function: no pub, no return type annotation.
fn emit_main_function(func: &Bullet) -> String {
    let mut out = String::new();

    // main() takes no arguments in Rust
    out.push_str("fn main() {\n");
    emit_body(&mut out, &func.body, &func.params, &Backend::Rust, true);
    out.push_str("}\n");
    out
}

fn emit_body(out: &mut String, body: &BulletBody, params: &[Param], backend: &Backend, returns_unit: bool) {
    match body {
        BulletBody::Pipes(pipes) => {
            let last = pipes.len().saturating_sub(1);
            for (i, pipe) in pipes.iter().enumerate() {
                let expr_str = match crate::pipe::classify(pipe) {
                    crate::pipe::PipeRhs::Builtin { name, args } => {
                        let params = crate::pipe::builtin_params(
                            args, &emit_expr, &crate::pipe::no_types,
                        );
                        match stdlib::emit_builtin(name, &params, backend) {
                            Ok(code) => code,
                            Err(e)   => format!("compile_error!(\"{e}\")"),
                        }
                    }
                    crate::pipe::PipeRhs::Call { name, args } => format!(
                        "{}({})",
                        name,
                        args.iter().map(emit_expr).collect::<Vec<_>>().join(", ")
                    ),
                    crate::pipe::PipeRhs::Expr(expr) => emit_expr(expr),
                };

                let binding = pipe.binding.as_deref().unwrap_or("_");
                if i == last {
                    if binding == "_" || pipe.binding.is_none() {
                        out.push_str(&format!("    {};\n", expr_str));
                    } else {
                        out.push_str(&format!("    let {} = {};\n", binding, expr_str));
                        out.push_str(&format!("    {}\n", binding));
                    }
        } else {
                    out.push_str(&format!("    let {} = {};\n", binding, expr_str));
                }
            }
        }
        BulletBody::Natives(blocks) => {
            let block = blocks.iter().find(|b| b.backend == Backend::Rust)
                .or_else(|| blocks.first());
            if let Some(b) = block {
                match b.backend {
                    Backend::Rust | Backend::Unknown(_) => {
                        if matches!(b.backend, Backend::Unknown(_)) {
                            out.push_str(&format!(
                                "    compile_error!(\"\'@{}\' is not a supported backend\")\n",
                                b.backend.escape_keyword()
                            ));
                        } else {
                            for line in b.code.lines() {
                                if line.trim().is_empty() { out.push('\n'); }
                                else { out.push_str(&format!("    {}\n", line)); }
                            }
                        }
                    }
                    _ => {
                        out.push_str("    compile_error!(\"no @rust block provided for this function\")\n");
                    }
                }
            }
        }
        BulletBody::Builtin(name) => {
            match stdlib::emit_builtin(name, params, backend) {
                // A unit-typed function (`-> ()`, including `main`) must not
                // place its sole builtin call in tail-expression position: a
                // builtin's emitted Rust code can evaluate to a non-unit
                // value (e.g. `close`'s `{ ...; 0i32 }`), which would then
                // conflict with the function's declared `()` return type.
                // A statement (semicolon, discarded) matches the same
                // reasoning as the last-pipe-with-no-binding case above.
                Ok(code) if returns_unit => out.push_str(&format!("    {};\n", code)),
                Ok(code)                 => out.push_str(&format!("    {}\n", code)),
                Err(e)                   => out.push_str(&format!("    compile_error!(\"{}\")\n", e)),
            }
        }
    }
}

// ── Expression emitters ───────────────────────────────────────────────────────

fn emit_expr(expr: &Expr) -> String {
    match expr {
        Expr::Atom(a)      => emit_atom(a),
        Expr::BinOp(b)     => format!("{} {} {}", emit_atom(&b.lhs), b.op, emit_atom(&b.rhs)),
        Expr::Tuple(exprs) => format!(
            "({})", exprs.iter().map(emit_expr).collect::<Vec<_>>().join(", ")
        ),
    }
}

fn emit_atom(atom: &Atom) -> String {
    match atom {
        Atom::Ident(s)            => s.clone(),
        Atom::Float(n) => n.to_string(),
        Atom::Integer(n)       => n.to_string(),
        Atom::StringLit(s)        => format!("\"{}\"", s),
        Atom::Interp(template)    => {
            // Rust 1.58+: format!("Hello {name}!") works directly with named captures.
            // Extract var names for the argument list.
            let vars = interp_vars(template);
            if vars.is_empty() {
                format!("\"{}\"", template)
            } else {
                // The template already uses {name} syntax — Rust format! accepts this.
                format!("format!(\"{}\")", template)
            }
        }
        Atom::Call { name, args } => {
            let args_str = args.iter().map(|a| match a {
                CallArg::Value(s) => s.clone(),
            }).collect::<Vec<_>>().join(", ");
            format!("{}({})", name, args_str)
        }
        Atom::BuiltinNoArgs(name) => format!(
            "compile_error!(\"'builtin::{name}' needs its arguments — give it its own bullet, \
             or call it as 'builtin::{name}(args)'\")"
        ),
        Atom::BuiltinExpr { name, args } =>
            match crate::pipe::inline_builtin(name, args, &Backend::Rust, &emit_expr) {
                Ok(code) => code,
                Err(e)   => format!("compile_error!(\"{e}\")"),
            },
        Atom::Unary { op, rhs } => format!("({}{})", op, emit_atom(rhs)),
        Atom::FieldAccess { base, fields } => format!("{}.{}", base, fields.join(".")),
        Atom::Index { base, idx } =>
            format!("{}.chars().nth(({}) as usize).unwrap_or('\\0')", base, emit_expr(idx)),
        // `from` is bound once rather than emitted twice: it may be a call,
        // and evaluating it twice would run it twice. The casts matter too —
        // skip and take want usize, and a bullet's indices are i64.
        Atom::Slice { base, from, to } => format!(
            "{{ let __from = ({}) as usize; let __to = ({}) as usize; \
             {}.chars().skip(__from).take(__to.saturating_sub(__from)).collect::<String>() }}",
            emit_expr(from), emit_expr(to), base
        ),
        Atom::EnumVariant { ty, variant } => format!("{}::{}", ty, variant),
    }
}

/// Extract all `{ident}` variable names from an interpolation template.
fn interp_vars(template: &str) -> Vec<&str> {
    let mut vars = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open+1..];
        if let Some(close) = after.find('}') {
            let name = &after[..close];
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                vars.push(name);
            }
            rest = &after[close+1..];
        } else { break; }
    }
    vars
}
