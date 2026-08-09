//! Go code generation backend.
//!
//! All files land in a single directory. When main.bu is present everything
//! uses `package main`; otherwise they use `package <crate>` as a library.
//!
//! Bullang snake_case names → Go PascalCase (exported). All functions public.
//!
//! Type mapping:
//!   i8/i16/i32/i64 → int8/int16/int32/int64
//!   u8/u16/u32/u64 → uint8/uint16/uint32/uint64
//!   usize/isize    → uint/int
//!   f32/f64        → float32/float64
//!   bool/char      → bool/rune
//!   String/&str    → string
//!   Vec<T>         → []T
//!   (T,U)          → struct{ V0 T; V1 U }
//!   Option<T>      → *T
//!   fn(T)->U       → func(T) U
//!   ()             → (omitted)

use bullang::ast::*;
use crate::codegen::typeinfer::{TypeEnv, collect_fn_output_types};
use std::collections::HashMap;

// ── Hoisted imports and helpers ─────────────────────────────────────────────

/// Go's import block. Three sources, and no guessing:
///
///   - what the file's builtins need, via the shared collector
///   - `fmt`, when a string template is emitted as `fmt.Sprintf`
///   - `#lib:` entries, which is how a project declares a Go import
///
/// The old version sniffed escape blocks for substrings like `"strings."` and
/// pushed `fmt` whenever any `@go` block existed at all. Both were guesses:
/// an escape block is opaque by design, so what it needs is exactly what
/// `#lib:` says it needs, and nothing else.
fn go_imports(file: &SourceFile, libs: &[String]) -> Vec<String> {
    let mut imports: Vec<String> = crate::codegen::hoist::requirements(file, &Backend::Go)
        .imports
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    for func in &file.bullets {
        if let BulletBody::Pipes(pipes) = &func.body {
            if pipes.iter().any(|p| pipe_has_interp(&p.expr)) {
                push_unique(&mut imports, "fmt".to_string());
            }
        }
    }
    for lib in libs {
        push_unique(&mut imports, lib.clone());
    }

    imports.sort();
    imports.dedup();
    imports
}

fn emit_go_imports(out: &mut String, file: &SourceFile, libs: &[String]) {
    let imports = go_imports(file, libs);
    if imports.is_empty() {
        return;
    }
    out.push_str("import (\n");
    for imp in &imports {
        out.push_str(&format!("\t\"{}\"\n", imp));
    }
    out.push_str(")\n\n");
}

fn emit_go_helpers(out: &mut String, file: &SourceFile) {
    for helper in crate::codegen::hoist::requirements(file, &Backend::Go).helpers {
        out.push_str(helper);
        out.push('\n');
    }
}

/// The helpers for a whole Go package, as one file.
///
/// Every other backend can hoist a helper into each file that needs it: Rust
/// and Python scope them per module, Java per class, and C's are `static`, so
/// each translation unit gets its own. Go has none of that — every file in a
/// package shares one namespace, so a two-file package that both printed
/// something declared `buOut` twice and did not compile.
///
/// Returns `None` when the package needs no helpers, so no empty file is
/// written.
pub fn emit_runtime_go(files: &[&SourceFile], package: &str) -> Option<String> {
    let mut helpers: Vec<&'static str> = Vec::new();
    let mut imports: Vec<&'static str> = Vec::new();
    for file in files {
        let hoisted = crate::codegen::hoist::requirements(file, &Backend::Go);
        for h in hoisted.helpers {
            if !helpers.contains(&h) { helpers.push(h); }
        }
        for i in hoisted.helper_imports {
            if !imports.contains(&i) { imports.push(i); }
        }
    }
    if helpers.is_empty() {
        return None;
    }
    imports.sort();

    let mut out = format!("package {}\n\n", sanitize_go_pkg(package));
    if !imports.is_empty() {
        out.push_str("import (\n");
        for i in &imports {
            out.push_str(&format!("\t\"{}\"\n", i));
        }
        out.push_str(")\n\n");
    }
    for h in helpers {
        out.push_str(h);
        out.push('\n');
    }
    Some(out)
}

// ── Source file → Go ──────────────────────────────────────────────────────────

pub fn emit_source_go(file: &SourceFile, package: &str, libs: &[String]) -> String {
    let pkg  = sanitize_go_pkg(package);
    let mut out = String::new();
    out.push_str(&format!("package {}\n\n", pkg));

    emit_go_imports(&mut out, file, libs);

    let fn_outputs = collect_fn_output_types(file);
    for func in &file.bullets {
        out.push_str(&emit_function_go(func, &fn_outputs));
        out.push('\n');
    }
    out
}

/// Bare single-file mode: only the function bodies, no package declaration,
/// no imports, no preamble.
pub fn emit_bare_go(file: &SourceFile) -> String {
    let mut out = String::new();
    emit_go_helpers(&mut out, file);
    let fn_outputs = collect_fn_output_types(file);
    for func in &file.bullets {
        out.push_str(&emit_function_go(func, &fn_outputs));
        out.push('\n');
    }
    out
}

/// Emit `types.go` — contains all inventory struct definitions and any
/// Tuple named structs needed as foreign type equivalents.
/// Called by build.rs whenever there are structs or Tuple types in the project.
pub fn emit_types_go(package: &str, structs: &[bullang::ast::StructDef], enums: &[bullang::ast::EnumDef], tuple_types: &[Vec<bullang::ast::BuType>]) -> String {
    let pkg = sanitize_go_pkg(package);
    let mut out = String::new();
    out.push_str(&format!("package {}\n\n", pkg));

    // Enum types — iota const blocks
    for e in enums {
        out.push_str(&emit_enum_go(e));
        out.push('\n');
    }

    for s in structs {
        out.push_str(&emit_struct_go(s));
        out.push('\n');
    }

    // Tuple foreign types — named structs derived from type combinations
    for inner in tuple_types {
        let type_name = tuple_go_name(inner);
        out.push_str(&format!("type {} struct {{\n", type_name));
        for (i, ty) in inner.iter().enumerate() {
            out.push_str(&format!("\tV{} {}\n", i, bu_type_to_go(ty)));
        }
        out.push_str("}\n\n");
    }

    out
}

/// Generate a stable Go type name for a Tuple from its inner types.
/// `Tuple[i32, f64]` → `Tuple_i32_f64`
pub fn tuple_go_name(inner: &[bullang::ast::BuType]) -> String {
    let parts: Vec<String> = inner.iter().map(|t| {
        bu_type_to_go(t)
            .replace(['<', '>', '[', ']', ' ', ','], "_")
            .trim_matches('_')
            .to_string()
    }).collect();
    format!("Tuple_{}", parts.join("_"))
}

/// Collect all unique Tuple type combinations used across all source files.
pub fn collect_tuple_types(source_files: &[(String, &SourceFile)]) -> Vec<Vec<bullang::ast::BuType>> {
    let mut seen: Vec<Vec<BuType>> = Vec::new();

    fn scan_type(ty: &BuType, seen: &mut Vec<Vec<BuType>>) {
        if let BuType::Tuple(inner) = ty {
            if !seen.contains(inner) {
                seen.push(inner.clone());
            }
        }
        if let BuType::Named(s) = ty {
            // Tuple[T, U] written as a Named variant
            if s.starts_with("Tuple[") && s.ends_with(']') {
                // parse inner types — handled at codegen via bu_type_to_go
                // just register the raw string as a single-element placeholder
            }
        }
    }

    for (_, sf) in source_files {
        for func in &sf.bullets {
            for param in &func.params { scan_type(&param.ty, &mut seen); }
            scan_type(&func.output.as_ref().map(|o| &o.ty).unwrap_or(&bullang::ast::BuType::unit()), &mut seen);
        }
    }
    seen
}

pub fn emit_struct_go(s: &bullang::ast::StructDef) -> String {
    let mut out = String::new();
    out.push_str(&format!("type {} struct {{\n", to_pascal_case(&s.name)));
    for field in &s.fields {
        out.push_str(&format!("\t{} {}\n",
            to_pascal_case(&field.name), bu_type_to_go(&field.ty)));
    }
    out.push_str("}\n");
    out
}

pub fn emit_enum_go(e: &bullang::ast::EnumDef) -> String {
    let mut out = String::new();
    out.push_str(&format!("type {} int\n\n", e.name));
    out.push_str("const (\n");
    for (i, v) in e.variants.iter().enumerate() {
        if i == 0 {
            out.push_str(&format!("\t{} {} = iota\n", v.name, e.name));
        } else {
            out.push_str(&format!("\t{}\n", v.name));
        }
    }
    out.push_str(")\n");
    out
}

// ── main.bu → main.go ────────────────────────────────────────────────────────

pub fn emit_main_go(file: &SourceFile, _package: &str, libs: &[String]) -> String {
    let mut out = String::new();
    out.push_str("package main\n\n");

    emit_go_imports(&mut out, file, libs);

    let fn_outputs = collect_fn_output_types(file);
    for func in &file.bullets {
        if func.name == "main" {
            out.push_str(&emit_main_function_go(func, &fn_outputs));
        } else {
            out.push_str(&emit_function_go(func, &fn_outputs));
        }
        out.push('\n');
    }
    out
}

/// Emit go.mod for the module.
pub fn emit_go_mod(module_name: &str) -> String {
    format!("module {}\n\ngo 1.21\n", module_name)
}

// ── Import detection ──────────────────────────────────────────────────────────

fn pipe_has_interp(expr: &bullang::ast::Expr) -> bool {
    match expr {
        Expr::Atom(Atom::Interp(_))     => true,
        Expr::Atom(_)                   => false,
        Expr::BinOp(b)                  => matches!(&b.lhs, Atom::Interp(_)) || matches!(&b.rhs, Atom::Interp(_)),
        Expr::Tuple(exprs)              => exprs.iter().any(pipe_has_interp),
    }
}

fn push_unique(v: &mut Vec<String>, s: String) {
    if !v.contains(&s) { v.push(s); }
}

// ── Function emitters ─────────────────────────────────────────────────────────

fn emit_function_go(func: &Bullet, fn_outputs: &HashMap<String, BuType>) -> String {
    let mut out   = String::new();
    let params    = go_param_list(&func.params);
    let ret       = bu_type_to_go(&func.output.as_ref().map(|o| &o.ty).unwrap_or(&bullang::ast::BuType::unit()));
    let go_name   = to_pascal_case(&func.name);

    let type_param_str = if func.type_params.is_empty() {
        String::new()
    } else {
        // Use constraints.Ordered if body uses comparison ops, any otherwise.
        let constraint = if go_needs_ordered(func) { "constraints.Ordered" } else { "any" };
        let tp = func.type_params.iter()
            .map(|t| format!("{} {}", t, constraint))
            .collect::<Vec<_>>().join(", ");
        format!("[{}]", tp)
    };

    if ret.is_empty() {
        out.push_str(&format!("func {}{}({}) {{\n", go_name, type_param_str, params));
    } else {
        out.push_str(&format!("func {}{}({}) {} {{\n", go_name, type_param_str, params, ret));
    }
    emit_body_go(&mut out, &func.body, &func.params, &func.output, fn_outputs);
    out.push_str("}\n");
    out
}

/// Returns true if the function body contains any comparison operator,
/// which requires the `constraints.Ordered` constraint in Go.
fn go_needs_ordered(func: &Bullet) -> bool {
    if let BulletBody::Pipes(pipes) = &func.body {
        pipes.iter().any(|p| go_expr_has_cmp(&p.expr))
    } else {
        false
    }
}

fn go_expr_has_cmp(expr: &Expr) -> bool {
    matches!(expr, Expr::BinOp(b) if matches!(b.op.as_str(), "<" | ">" | "<=" | ">="))
}

fn emit_main_function_go(func: &Bullet, fn_outputs: &HashMap<String, BuType>) -> String {
    let mut out = String::new();
    out.push_str("func main() {\n");
    emit_body_go(&mut out, &func.body, &func.params, &func.output, fn_outputs);
    out.push_str("}\n");
    out
}

fn emit_body_go(out: &mut String, body: &BulletBody, params: &[Param], output: &Option<OutputDecl>, fn_outputs: &HashMap<String, BuType>) {
    match body {
        BulletBody::Pipes(pipes) => {
            if pipes.is_empty() { return; }
            let last = pipes.len().saturating_sub(1);
            let mut env = TypeEnv::seed(params, fn_outputs);
            // Counter has to be unique across the whole function body, not
            // reset per pipe: Go's `:=` requires at least one new variable
            // on the left side, so reusing `__arg_0` for a second pipe
            // in the same function is a hard compile error ("no new
            // variables on left side of :="), unlike Python/Java where
            // reassignment is fine either way.
            let mut tmp_counter: usize = 0;
            for (i, pipe) in pipes.iter().enumerate() {
                // Handle builtin::name with implicit pipe inputs
                let expr_str = if let Expr::Atom(Atom::BuiltinNoArgs(name)) = &pipe.expr {
                    let synthetic_params: Vec<bullang::ast::Param> = pipe.inputs
                        .iter()
                        .map(|input| {
                            let inferred_ty = env.infer(input);
                            let param_name = match input {
                                Expr::Atom(Atom::Ident(s)) => s.clone(),
                                // Spliced straight in. A hoisted `__arg := 1`
                                // takes Go's default `int`, which is not the
                                // `int64` the shim declares; an untyped
                                // constant in argument position converts on
                                // its own.
                                Expr::Atom(Atom::Integer(_))
                                | Expr::Atom(Atom::Float(_))
                                | Expr::Atom(Atom::StringLit(_)) => emit_expr_go(input),
                                // Not a plain variable — declare a real
                                // temporary above the call and reference
                                // that instead. Previously this fell back to
                                // a made-up `__pipe_arg_N` name with no
                                // matching declaration anywhere, so any
                                // multi-arg implicit call with a non-ident
                                // input (e.g. `(path, "w"): builtin::open`)
                                // produced Go that referenced an undefined
                                // variable — see codegen_c.rs's equivalent
                                // block, which already does this correctly.
                                _ => {
                                    let tmp = format!("__arg_{}", tmp_counter);
                                    tmp_counter += 1;
                                    out.push_str(&format!(
                                        "\t{} := {}\n",
                                        tmp, emit_expr_go(input)
                                    ));
                                    tmp
                                }
                            };
                            bullang::ast::Param {
                                name: param_name,
                                ty:   inferred_ty,
                            }
                        })
                        .collect();
                    match crate::stdlib::emit_builtin(name, &synthetic_params, &Backend::Go) {
                        Ok(code) => code,
                        Err(e)   => format!("/* ERROR: {e} */"),
                    }
                } else {
                    // The pipe's inputs are arguments only when the expression
                    // is a bare callee. `a + b` is complete on its own; so is
                    // `some_fn(x, 2)`. Appending the inputs to either — which
                    // is what every backend used to do — produced `a + b(a, b)`.
                    match crate::pipe::classify(pipe) {
                        crate::pipe::PipeRhs::Call { name, args } => format!(
                            "{}({})",
                            to_pascal_case(name),
                            args.iter().map(emit_expr_go).collect::<Vec<_>>().join(", ")
                        ),
                        crate::pipe::PipeRhs::Expr(expr) => emit_expr_go(expr),
                        // Handled above, before this branch is reached.
                        crate::pipe::PipeRhs::Builtin { .. } => unreachable!(),
                    }
                };
                if let Some(binding) = pipe.binding.as_deref() {
                    env.bind(binding, env.infer(&pipe.expr));
                }
                match pipe.binding.as_deref() {
                    Some(name) if !name.is_empty() && name != "_" =>
                        out.push_str(&format!("\t{} := {}\n", name, expr_str)),
                    // `-> {}` discards. `_ := expr` is a compile error in Go;
                    // the bare call is what a reader would write anyway.
                    _ => out.push_str(&format!("\t{}\n", expr_str)),
                }
                if i == last {
                    let ret = output.as_ref().map(|o| bu_type_to_go(&o.ty)).unwrap_or_default();
                    if !ret.is_empty() {
                        out.push_str(&format!("\treturn {}\n", pipe.binding.as_deref().unwrap_or("_")));
                    }
                }
            }
        }
        BulletBody::Natives(blocks) => {
            let block = blocks.iter().find(|b| b.backend == Backend::Go);
            match block {
                Some(b) => {
                    let base = b.code.lines()
                        .filter(|l| !l.trim().is_empty())
                        .map(|l| l.len() - l.trim_start().len())
                        .min().unwrap_or(0);
                    for line in b.code.lines() {
                        if line.trim().is_empty() { out.push('\n'); }
                        else {
                            let stripped = if line.len() >= base { &line[base..] }
                                           else { line.trim_start() };
                            out.push_str(&format!("\t{}\n", stripped));
                        }
                    }
                }
                None => {
                    if let Some(b) = blocks.first() {
                        out.push_str(&format!(
                            "\t// ERROR: '@{}' cannot be used in a Go build — use '@go'\n",
                            b.backend.escape_keyword()
                        ));
                    }
                }
            }
        }
        BulletBody::Builtin(name) => {
            use crate::stdlib;
            match stdlib::emit_builtin(name, params, &Backend::Go) {
                Ok(code) => {
                    let ret = output.as_ref().map(|o| bu_type_to_go(&o.ty)).unwrap_or_default();
                    if ret.is_empty() {
                        out.push_str(&format!("\t{}\n", code));
                    } else {
                        // Cast to declared return type — builtins may return int64
                        // while the function declares int32, float32, etc.
                        out.push_str(&format!("\treturn {}({})\n", ret, code));
                    }
                }
                Err(e) => out.push_str(&format!("\t// ERROR: {}\n", e)),
            }
        }
    }
}

// ── Expression emitters ───────────────────────────────────────────────────────

fn emit_expr_go(expr: &Expr) -> String {
    match expr {
        Expr::Atom(a)      => emit_atom_go(a),
        Expr::BinOp(b)     => format!("{} {} {}",
            emit_atom_go(&b.lhs), b.op, emit_atom_go(&b.rhs)),
        Expr::Tuple(exprs) => {
            format!("struct{{ {} }}{{{}}}",
                exprs.iter().enumerate()
                    .map(|(i, _)| format!("V{} interface{{}}", i))
                    .collect::<Vec<_>>().join("; "),
                exprs.iter().map(emit_expr_go).collect::<Vec<_>>().join(", "))
        }
    }
}

fn emit_atom_go(atom: &Atom) -> String {
    match atom {
        Atom::Ident(s)         => s.clone(),
        Atom::Float(n) => n.to_string(),
        Atom::Integer(n)       => n.to_string(),
        Atom::StringLit(s)     => format!("\"{}\"", s),
        Atom::BuiltinNoArgs(name) => format!(
            "/* ERROR: 'builtin::{name}' needs its arguments — give it its own \
             bullet, or call it as 'builtin::{name}(args)' */"
        ),
        Atom::BuiltinExpr { name, args } =>
            match crate::pipe::inline_builtin(name, args, &Backend::Go, &emit_expr_go) {
                Ok(code) => code,
                Err(e)   => format!("/* ERROR: {e} */"),
            },
        Atom::Interp(template) => {
            // Go uses fmt.Sprintf with %v for each interpolated variable.
            let (fmt_str, vars) = interp_to_sprintf(template);
            if vars.is_empty() {
                format!("\"{}\"", fmt_str)
            } else {
                format!("fmt.Sprintf(\"{}\", {})", fmt_str, vars.join(", "))
            }
        }
        Atom::Call { name, args } => {
            let go_name  = to_pascal_case(name);
            let args_str = args.iter().map(|a| match a {
                CallArg::Value(s) => s.clone(),
            }).collect::<Vec<_>>().join(", ");
            format!("{}({})", go_name, args_str)
        }
        Atom::Unary { op, rhs } => format!("({}{})", op, emit_atom_go(rhs)),
        Atom::FieldAccess { base, fields } => {
            let pascal_fields: Vec<String> = fields.iter().map(|f| to_pascal_case(f)).collect();
            format!("{}.{}", base, pascal_fields.join("."))
        }
        Atom::Index { base, idx } =>
            format!("string([]rune({})[{}])", base, emit_expr_go(idx)),
        Atom::Slice { base, from, to } =>
            format!("string([]rune({})[{}:{}])", base, emit_expr_go(from), emit_expr_go(to)),
        // Go enum constants are package-level — emit bare name
        Atom::EnumVariant { variant, .. } => variant.clone(),
    }
}
/// `"Hello {name}!"` → `("Hello %v!", ["name"])`
fn interp_to_sprintf(template: &str) -> (String, Vec<&str>) {
    let mut fmt_str = String::new();
    let mut vars    = Vec::new();
    let mut rest    = template;
    while !rest.is_empty() {
        if let Some(open) = rest.find('{') {
            fmt_str.push_str(&rest[..open]);
            let after = &rest[open+1..];
            if let Some(close) = after.find('}') {
                let name = &after[..close];
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    fmt_str.push_str("%v");
                    vars.push(name);
                    rest = &after[close+1..];
                } else {
                    fmt_str.push('{');
                    rest = after;
                }
            } else {
                fmt_str.push_str(&rest[open..]);
                break;
            }
        } else {
            fmt_str.push_str(rest);
            break;
        }
    }
    (fmt_str, vars)
}

// ── Type mapping ──────────────────────────────────────────────────────────────

pub fn bu_type_to_go(ty: &BuType) -> String {
    match ty {
        BuType::Named(s)     => rust_type_to_go(s),
        BuType::Tuple(inner) => tuple_go_name(inner),
        BuType::Unknown      => "interface{}".to_string(),
    }
}

fn rust_type_to_go(s: &str) -> String {
    let s: String = s.split_whitespace().collect();
    match s.as_str() {
        "i8"    => "int8".to_string(),
        "i16"   => "int16".to_string(),
        "i32"   => "int32".to_string(),
        "i64"   => "int64".to_string(),
        "i128"  => "int64".to_string(),
        "isize" => "int".to_string(),
        "u8"    => "uint8".to_string(),
        "u16"   => "uint16".to_string(),
        "u32"   => "uint32".to_string(),
        "u64"   => "uint64".to_string(),
        "u128"  => "uint64".to_string(),
        "usize" => "uint".to_string(),
        "f32"   => "float32".to_string(),
        "f64"   => "float64".to_string(),
        "bool"  => "bool".to_string(),
        "char"  => "rune".to_string(),
        "String" | "&str" => "string".to_string(),
        "()"    => String::new(),
        other   => translate_go_generic(other),
    }
}

fn translate_go_generic(s: &str) -> String {
    if s.starts_with("Vec[") && s.ends_with(']') {
        return format!("[]{}", rust_type_to_go(&s[4..s.len()-1]));
    }
    if s.starts_with("HashMap[") && s.ends_with(']') {
        let inner = &s[8..s.len()-1];
        let parts: Vec<&str> = inner.splitn(2, ',').collect();
        if parts.len() == 2 {
            return format!("map[{}]{}",
                rust_type_to_go(parts[0].trim()),
                rust_type_to_go(parts[1].trim()));
        }
    }
    if s.starts_with("Option[") && s.ends_with(']') {
        return format!("*{}", rust_type_to_go(&s[7..s.len()-1]));
    }
    if s.starts_with('&') {
        return format!("*{}", rust_type_to_go(s[1..].trim()));
    }
    if s.starts_with("Fn[") {
        return translate_fn_type_go(s);
    }
    format!("interface{{}}  /* {} */", s)
}

fn translate_fn_type_go(s: &str) -> String {
    // Fn[T, U -> V]  →  func(T, U) V
    let inner = s.trim_start_matches("Fn[").trim_end_matches(']');
    if inner.is_empty() { return "func()".to_string(); }
    if let Some(arrow) = inner.find("->") {
        let params_str = inner[..arrow].trim();
        let ret_str    = inner[arrow+2..].trim();
        let params: Vec<String> = if params_str.is_empty() { vec![] }
            else { params_str.split(',').map(|p| rust_type_to_go(p.trim())).collect() };
        let ret = rust_type_to_go(ret_str);
        if ret.is_empty() { format!("func({})", params.join(", ")) }
        else { format!("func({}) {}", params.join(", "), ret) }
    } else {
        let ret = rust_type_to_go(inner.trim());
        format!("func() {}", ret)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn go_param_list(params: &[Param]) -> String {
    params.iter()
        .map(|p| format!("{} {}", p.name, bu_type_to_go(&p.ty)))
        .collect::<Vec<_>>().join(", ")
}

/// Convert snake_case or camelCase to PascalCase for Go export convention.
pub fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut next_upper = true;
    for ch in s.chars() {
        if ch == '_' {
            next_upper = true;
        } else if next_upper {
            result.extend(ch.to_uppercase());
            next_upper = false;
        } else {
            result.push(ch);
        }
    }
    result
}

pub fn sanitize_go_pkg(name: &str) -> String {
    let lower: String = name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    // Remove leading underscores/digits
    lower.trim_matches(|c: char| !c.is_ascii_alphabetic()).to_string()
}
