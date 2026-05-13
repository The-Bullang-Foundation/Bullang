//! C code generation backend.
//!
//! Produces a self-contained C source file per Bullang source file,
//! a shared header (<crate>.h) that exposes all public functions,
//! and a Makefile to compile the project.

use crate::ast::*;

// ── Source file → C ───────────────────────────────────────────────────────────

pub fn emit_source_c(file: &SourceFile, header_name: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("#include \"{}\"\n", header_name));
    out.push_str("#include <stdlib.h>\n");
    out.push_str("#include <string.h>\n\n");

    for func in &file.bullets {
        out.push_str(&emit_function_c(func));
        out.push('\n');
    }
    out
}

// ── Struct emitter ────────────────────────────────────────────────────────────

pub fn emit_struct_c(s: &crate::ast::StructDef) -> String {
    let mut out = String::new();
    out.push_str(&format!("typedef struct {{\n"));
    for field in &s.fields {
        out.push_str(&format!("    {} {};\n", bu_type_to_c(&field.ty), field.name));
    }
    out.push_str(&format!("}} {};\n", s.name));
    out
}

pub fn emit_enum_c(e: &crate::ast::EnumDef) -> String {
    let mut out = String::new();
    out.push_str("typedef enum {\n");
    for v in &e.variants {
        out.push_str(&format!("    {},\n", v.name));
    }
    out.push_str(&format!("}} {};\n", e.name));
    out
}

// ── foreign_types.h detection ─────────────────────────────────────────────────

/// Returns true if the source file uses any type that requires foreign_types.h.
pub fn needs_foreign_types(file: &SourceFile) -> bool {
    file.bullets.iter().any(|b| {
        b.params.iter().any(|p| type_needs_foreign(&p.ty))
            || type_needs_foreign(&b.output.ty)
    })
}

pub fn needs_generic_types(file: &SourceFile) -> bool {
    file.bullets.iter().any(|b| !b.type_params.is_empty())
}

fn type_needs_foreign(ty: &BuType) -> bool {
    match ty {
        BuType::Named(s) => s.starts_with("Vec[") || s.starts_with("HashMap["),
        BuType::Array(t, _) => type_needs_foreign(t),
        BuType::Tuple(ts)   => ts.iter().any(type_needs_foreign),
        BuType::Unknown     => false,
    }
}

pub fn emit_header_c(
    module_name:  &str,
    source_files: &[(String, &SourceFile)],
    includes:     &[String],
    structs:      &[crate::ast::StructDef],
    enums:        &[crate::ast::EnumDef],
) -> String {
    let guard    = format!("{}_H", module_name.to_uppercase().replace('-', "_"));
    let needs_ft  = source_files.iter().any(|(_, sf)| needs_foreign_types(sf));
    let needs_gen = source_files.iter().any(|(_, sf)| needs_generic_types(sf));
    let mut out   = String::new();

    out.push_str(&format!("#ifndef {}\n#define {}\n\n", guard, guard));
    out.push_str("#include <stdint.h>\n");
    out.push_str("#include <stdbool.h>\n");
    out.push_str("#include <stddef.h>\n");
    if needs_ft {
        out.push_str("#include \"foreign_types.h\"\n");
    }
    if needs_gen {
        out.push_str("#include \"bu_generic.h\"\n");
    }
    for inc in includes {
        out.push_str(&format!("#include <{}>\n", inc));
    }
    out.push('\n');

    // Enum typedefs — variants land in global scope (C enum semantics)
    for e in enums {
        out.push_str(&emit_enum_c(e));
        out.push('\n');
    }

    // Inventory struct typedefs
    for s in structs {
        out.push_str(&emit_struct_c(s));
        out.push('\n');
    }

    for (filename, sf) in source_files {
        out.push_str(&format!("/* {} */\n", filename));
        for func in &sf.bullets {
            let params = c_param_list(&func.params);
            let ret    = bu_type_to_c(&func.output.ty);
            out.push_str(&format!("{} {}({});\n", ret, func.name, params));
        }
        out.push('\n');
    }

    out.push_str(&format!("#endif /* {} */\n", guard));
    out
}

// ── main.bu → main.c ─────────────────────────────────────────────────────────

pub fn emit_main_c(file: &SourceFile, header_name: &str) -> String {
    let mut out = String::new();
    out.push_str("#include <stdio.h>\n");
    out.push_str("#include <stdlib.h>\n");
    out.push_str(&format!("#include \"{}\"\n\n", header_name));

    for func in &file.bullets {
        if func.name == "main" {
            out.push_str(&emit_main_function_c(func));
        } else {
            out.push_str(&emit_function_c(func));
        }
        out.push('\n');
    }
    out
}

/// Emit a Makefile for the generated C project.
pub fn emit_makefile(
    crate_name:   &str,
    source_files: &[String],
    has_main:     bool,
) -> String {
    let objects: Vec<String> = source_files.iter()
        .map(|f| f.replace(".c", ".o"))
        .collect();
    let obj_str = objects.join(" ");

    let mut out = String::new();
    out.push_str("CC      = cc\n");
    out.push_str("CFLAGS  = -Wall -Werror -Wextra -g -std=c11\n");
    out.push_str(&format!("TARGET  = {}\n\n", crate_name));
    out.push_str(&format!("OBJECTS = {}\n\n", obj_str));

    if has_main {
        out.push_str("all: $(TARGET)\n\n");
        out.push_str("$(TARGET): $(OBJECTS)\n");
        out.push_str("\t$(CC) $(CFLAGS) -o $@ $^\n\n");
    } else {
        out.push_str(&format!("all: lib{}.a\n\n", crate_name));
        out.push_str(&format!("lib{}.a: $(OBJECTS)\n", crate_name));
        out.push_str("\tar rcs $@ $^\n\n");
    }

    out.push_str("%.o: %.c\n");
    out.push_str("\t$(CC) $(CFLAGS) -c -o $@ $<\n\n");

    out.push_str("clean:\n");
    out.push_str(&format!("\trm -f $(OBJECTS) $(TARGET) lib{}.a\n\n", crate_name));

    out.push_str(".PHONY: all clean\n");
    out
}

// ── Function emitters ─────────────────────────────────────────────────────────

fn emit_function_c(func: &Bullet) -> String {
    let mut out = String::new();

    if func.type_params.is_empty() {
        let params = c_param_list(&func.params);
        let ret    = bu_type_to_c(&func.output.ty);
        out.push_str(&format!("{} {}({}) {{\n", ret, func.name, params));
        emit_body_c(&mut out, &func.body, &func.params, &Backend::C);
    } else {
        // Generic function — type params become BuVal.
        out.push_str("#include \"bu_generic.h\"\n");
        let params = c_generic_param_list(&func.params, &func.type_params);
        let ret    = c_generic_type(&func.output.ty, &func.type_params);
        out.push_str(&format!("{} {}({}) {{\n", ret, func.name, params));
        emit_body_c_generic(&mut out, &func.body, &func.type_params);
    }

    out.push_str("}\n");
    out
}

/// Param list for a generic C function: type params → BuVal, concrete types unchanged.
fn c_generic_param_list(params: &[Param], type_params: &[String]) -> String {
    params.iter()
        .map(|p| format!("{} {}", c_generic_type(&p.ty, type_params), p.name))
        .collect::<Vec<_>>().join(", ")
}

/// Map a type to its C representation — type param names become BuVal.
fn c_generic_type(ty: &BuType, type_params: &[String]) -> String {
    match ty {
        BuType::Named(s) if type_params.contains(s) => "BuVal".to_string(),
        other => bu_type_to_c(other),
    }
}

/// Emit a function body where type-param-typed values are BuVal.
/// All binary ops use bu_val_* dispatch; integer/float literals are wrapped.
fn emit_body_c_generic(out: &mut String, body: &BulletBody, type_params: &[String]) {
    match body {
        BulletBody::Pipes(pipes) => {
            if pipes.is_empty() { return; }
            let last = pipes.len().saturating_sub(1);
            for (i, pipe) in pipes.iter().enumerate() {
                let expr_str = emit_expr_c_generic(&pipe.expr, type_params);
                if i == last {
                    out.push_str(&format!("    return {};\n", expr_str));
                } else {
                    out.push_str(&format!("    BuVal {} = {};\n", pipe.binding, expr_str));
                }
            }
        }
        BulletBody::Natives(blocks) => {
            // Native blocks in a generic function are emitted verbatim — user takes
            // responsibility for using BuVal correctly.
            if let Some(b) = blocks.iter().find(|b| b.backend == Backend::C || b.backend == Backend::Cpp) {
                for line in b.code.lines() {
                    out.push_str(&format!("    {}\n", line));
                }
            }
        }
        BulletBody::Builtin(name) => {
            out.push_str(&format!("    /* builtin::{} in generic context */\n", name));
        }
    }
}

/// Expression emitter for generic C functions — all ops route through bu_val_*.
fn emit_expr_c_generic(expr: &Expr, tp: &[String]) -> String {
    match expr {
        Expr::Atom(a)  => emit_atom_c_generic(a, tp),
        Expr::BinOp(b) => {
            let l = emit_atom_c_generic(&b.lhs, tp);
            let r = emit_atom_c_generic(&b.rhs, tp);
            let fn_name = match b.op.as_str() {
                "+"  => "bu_val_add",
                "-"  => "bu_val_sub",
                "*"  => "bu_val_mul",
                "/"  => "bu_val_div",
                "%"  => "bu_val_mod",
                "==" => "bu_val_eq",
                "!=" => "bu_val_ne",
                "<"  => "bu_val_lt",
                ">"  => "bu_val_gt",
                "<=" => "bu_val_le",
                ">=" => "bu_val_ge",
                "&&" => "bu_val_and",
                "||" => "bu_val_or",
                op   => return format!("({} {} {})", l, op, r),
            };
            format!("{}({}, {})", fn_name, l, r)
        }
        Expr::Tuple(exprs) => {
            // Tuples in generic context: emit as first element (no tuple type in C).
            exprs.first().map(|e| emit_expr_c_generic(e, tp))
                .unwrap_or_else(|| "bu_i64(0)".to_string())
        }
    }
}

/// Atom emitter for generic C functions — wraps literals as BuVal.
fn emit_atom_c_generic(atom: &Atom, tp: &[String]) -> String {
    match atom {
        Atom::Integer(n)  => format!("bu_i64({})", n),
        Atom::Float(n)    => format!("bu_f64({})", n),
        Atom::StringLit(s) => format!("bu_str(\"{}\")", s),
        Atom::Ident(s)    => s.clone(), // already BuVal if it was a type-param param
        Atom::Unary { op, rhs } => {
            let r = emit_atom_c_generic(rhs, tp);
            if op == "-" { format!("bu_val_neg({})", r) }
            else         { format!("bu_val_not({})", r) }
        }
        Atom::EnumVariant { variant, .. } => format!("bu_i64({})", variant),
        // For non-generic atoms, fall back to the regular C emitter.
        other => emit_atom_c(other),
    }
}

fn emit_main_function_c(func: &Bullet) -> String {
    let mut out = String::new();
    out.push_str("int main(void) {\n");
    emit_body_c(&mut out, &func.body, &func.params, &Backend::C);
    // If body doesn't have a return, add one
    out.push_str("    return 0;\n");
    out.push_str("}\n");
    out
}

pub fn emit_body_c(out: &mut String, body: &BulletBody, params: &[Param], backend: &Backend) {
    match body {
        BulletBody::Pipes(pipes) => {
            if pipes.is_empty() { return; }
            let last = pipes.len().saturating_sub(1);
            for (i, pipe) in pipes.iter().enumerate() {
                let expr_str = emit_expr_c(&pipe.expr);
                if i == last {
                    out.push_str(&format!("    return {};\n", expr_str));
                } else {
                    out.push_str(&format!("    __auto_type {} = {};\n", pipe.binding, expr_str));
                    if pipe.propagate {
                        out.push_str(&format!(
                            "    if (!{}) {{ return NULL; }}\n",
                            pipe.binding
                        ));
                    }
                }
            }
        }
        BulletBody::Natives(blocks) => {
            let block = blocks.iter()
                .find(|b| b.backend == *backend || b.backend == Backend::C || b.backend == Backend::Cpp);
            match block {
                Some(b) => {
                    let base_indent = b.code.lines()
                        .filter(|l| !l.trim().is_empty())
                        .map(|l| l.len() - l.trim_start_matches(' ').len())
                        .min().unwrap_or(0);
                    for line in b.code.lines() {
                        if line.trim().is_empty() { out.push('\n'); }
                        else {
                            let stripped = if line.len() >= base_indent { &line[base_indent..] } else { line.trim_start() };
                            out.push_str(&format!("    {}\n", stripped));
                        }
                    }
                }
                None => {
                    if let Some(b) = blocks.first() {
                        out.push_str(&format!(
                            "    /* ERROR: '@{}' block cannot compile to C */\n",
                            b.backend.escape_keyword()
                        ));
                    }
                }
            }
        }
        BulletBody::Builtin(name) => {
            use crate::stdlib;
            match stdlib::emit_builtin(name, params, backend) {
                Ok(code) => out.push_str(&format!("    return {};\n", code)),
                Err(e)   => out.push_str(&format!("    /* ERROR: {} */\n", e)),
            }
        }
    }
}

// ── Expression emitters ───────────────────────────────────────────────────────

pub fn emit_expr_c(expr: &Expr) -> String {
    match expr {
        Expr::Atom(a)      => emit_atom_c(a),
        Expr::BinOp(b)     => format!("{} {} {}", emit_atom_c(&b.lhs), b.op, emit_atom_c(&b.rhs)),
        Expr::Tuple(exprs) => {
            // C has no tuple type — emit as a struct initialiser comment
            format!("/* tuple: {} */",
                exprs.iter().map(emit_expr_c).collect::<Vec<_>>().join(", "))
        }
    }
}

pub fn emit_atom_c(atom: &Atom) -> String {
    match atom {
        Atom::Ident(s)         => s.clone(),
        Atom::Float(n) => n.to_string(),
        Atom::Integer(n)       => n.to_string(),
        Atom::StringLit(s)     => format!("\"{}\"", s),
        Atom::BuiltinExpr { name, args } => {
            match name.as_str() {
                "assert" => {
                    let cond = emit_expr_c(&args[0]);
                    format!(
                        "({{ int __r = (int)({cond}); \
                         if (!__r) {{ fprintf(stderr, \"[assert] failed\\n\"); }} \
                         __r; }})"
                    )
                }
                "assert_eq" => {
                    let lhs = emit_expr_c(&args[0]);
                    let rhs = emit_expr_c(&args[1]);
                    format!(
                        "({{ int __ok = (({lhs}) == ({rhs})); \
                         if (!__ok) {{ fprintf(stderr, \"[assert_eq] failed\\n\"); }} \
                         __ok; }})"
                    )
                }
                "assert_ne" => {
                    let lhs = emit_expr_c(&args[0]);
                    let rhs = emit_expr_c(&args[1]);
                    format!(
                        "({{ int __ok = (({lhs}) != ({rhs})); \
                         if (!__ok) {{ fprintf(stderr, \"[assert_ne] values were equal\\n\"); }} \
                         __ok; }})"
                    )
                }
                other => format!("0 /* builtin::{other} not supported as expression */"),
            }
        }
        Atom::Interp(template) => {
            // C/C++: produce a snprintf call into a stack buffer.
            // "Hello {name}!" → snprintf(buf, sizeof(buf), "Hello %s!", name)
            let (fmt_str, vars) = interp_to_printf(template);
            if vars.is_empty() {
                format!("\"{}\"", fmt_str)
            } else {
                let args = vars.join(", ");
                // Emit as a compound-literal char array expression.
                // Caller is responsible for storage if used as an lvalue.
                format!("({{ static char _buf[1024]; snprintf(_buf, sizeof(_buf), \"{}\", {}); _buf; }})",
                    fmt_str, args)
            }
        }
        Atom::Call { name, args } => {
            let args_str = args.iter().map(|a| match a {
                CallArg::Value(s)     => s.clone(),
                CallArg::BulletRef(s) => s.clone(),
            }).collect::<Vec<_>>().join(", ");
            format!("{}({})", name, args_str)
        }
        Atom::Unary { op, rhs } => format!("({}{})", op, emit_atom_c(rhs)),
        Atom::FieldAccess { base, fields } => format!("{}.{}", base, fields.join(".")),
        Atom::Index { base, idx } =>
            format!("{}[{}]", base, emit_expr_c(idx)),
        Atom::Slice { base, from, to } =>
            format!("strndup(({}) + ({}), (size_t)(({})-({0})))",
                base, emit_expr_c(from), emit_expr_c(to)),
        // C typedef enum: variants are in global scope — emit bare variant name
        Atom::EnumVariant { variant, .. } => variant.clone(),
    }
}
/// `"Hello {name}!"` → `("Hello %s!", ["name"])`
fn interp_to_printf(template: &str) -> (String, Vec<&str>) {
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
                    fmt_str.push_str("%s");
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

// ── Type mapping: Bullang → C ─────────────────────────────────────────────────

pub fn bu_type_to_c(ty: &BuType) -> String {
    match ty {
        BuType::Named(s)     => rust_type_to_c(s),
        BuType::Tuple(_)     => "void*  /* tuple — use a struct */".to_string(),
        BuType::Array(t, n)  => format!("{}[{}]", bu_type_to_c(t), n),
        BuType::Unknown      => "void*".to_string(),
    }
}

fn rust_type_to_c(s: &str) -> String {
    let s: String = s.split_whitespace().collect();
    match s.as_str() {
        "i8"    => "int8_t".to_string(),
        "i16"   => "int16_t".to_string(),
        "i32"   => "int32_t".to_string(),
        "i64"   => "int64_t".to_string(),
        "i128"  => "__int128".to_string(),
        "isize" => "ptrdiff_t".to_string(),
        "u8"    => "uint8_t".to_string(),
        "u16"   => "uint16_t".to_string(),
        "u32"   => "uint32_t".to_string(),
        "u64"   => "uint64_t".to_string(),
        "u128"  => "unsigned __int128".to_string(),
        "usize" => "size_t".to_string(),
        "f32"   => "float".to_string(),
        "f64"   => "double".to_string(),
        "bool"  => "bool".to_string(),
        "char"  => "char".to_string(),
        "String" | "&str" => "char*".to_string(),
        "()"    => "void".to_string(),
        other   => translate_c_generic(other),
    }
}

fn translate_c_generic(s: &str) -> String {
    // Vec[T] → vec_t  (foreign_types.h dynamic array)
    if s.starts_with("Vec[") && s.ends_with(']') {
        return "vec_t".to_string();
    }
    // HashMap[K, V] → map_t  (foreign_types.h hash map, string keys)
    if s.starts_with("HashMap[") && s.ends_with(']') {
        return "map_t".to_string();
    }
    // &T → T*
    if s.starts_with('&') {
        let inner = s[1..].trim();
        return format!("{}*", rust_type_to_c(inner));
    }
    // &mut T → T*
    if s.starts_with("&mut") {
        let inner = s[4..].trim();
        return format!("{}*", rust_type_to_c(inner));
    }
    // Option<T> → T*  (nullable pointer)
    if s.starts_with("Option[") && s.ends_with(']') {
        let inner = &s[7..s.len()-1];
        return format!("{}*  /* nullable */", rust_type_to_c(inner));
    }
    // Fn[...] → function pointer
    if s.starts_with("Fn[") {
        return "void*  /* fn ptr */".to_string();
    }
    // Bare type parameter (e.g. T, K, V, E) in a non-generic context — shouldn't
    // normally occur; pass through with a comment.
    if s.chars().all(|c| c.is_alphabetic()) && s.len() <= 2 {
        return "BuVal  /* generic type param */".to_string();
    }
    // Unknown: pass through
    format!("{}  /* ? */", s)
}

fn c_param_list(params: &[Param]) -> String {
    if params.is_empty() { return "void".to_string(); }
    params.iter()
        .map(|p| format!("{} {}", bu_type_to_c(&p.ty), p.name))
        .collect::<Vec<_>>().join(", ")
}
