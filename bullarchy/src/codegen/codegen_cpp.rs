//! C++ code generation backend.
//!
//! Produces a C++ source file (.cpp) per Bullang source file,
//! a shared header (.hpp) with declarations inside a namespace,
//! and a Makefile.

use bullang::ast::*;
use crate::codegen::codegen_c;
use std::collections::BTreeSet;

// ── Hoisted includes / helper functions ─────────────────────────────────────

/// Same pattern as codegen_c::collect_c_includes, for the C++ backend.
/// Helpers are free functions, not namespace-scoped, so a caller inside
/// `namespace ns { ... }` still resolves them by ordinary unqualified lookup
/// into the enclosing global scope.
fn emit_cpp_includes(out: &mut String, file: &SourceFile) {
    for include in crate::codegen::hoist::requirements(file, &Backend::Cpp).imports {
        out.push_str(include);
        out.push('\n');
    }
}

fn emit_cpp_helpers(out: &mut String, file: &SourceFile) {
    for helper in crate::codegen::hoist::requirements(file, &Backend::Cpp).helpers {
        out.push_str(helper);
        out.push('\n');
    }
}

// ── Source file → C++ ────────────────────────────────────────────────────────

pub fn emit_source_cpp(file: &SourceFile, header_name: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("#include \"{}\"\n", header_name));
    out.push_str("#include <cstdlib>\n");
    out.push_str("#include <cstring>\n");
    emit_cpp_includes(&mut out, file);
    out.push('\n');
    emit_cpp_helpers(&mut out, file);

    let unit_fns = codegen_c::collect_unit_functions(file);
    let ns = header_name.trim_end_matches(".hpp");
    out.push_str(&format!("namespace {} {{\n\n", ns));
    for func in &file.bullets {
        out.push_str(&emit_function_cpp(func, &unit_fns));
        out.push('\n');
    }
    out.push_str(&format!("}} // namespace {}\n", ns));
    out
}

/// Single-file mode: emit a self-contained `.cpp` with no companion `.hpp`.
/// Bare single-file mode: only the function bodies plus any hoisted builtin
/// includes/helpers — no other preamble.
pub fn emit_bare_cpp(file: &SourceFile) -> String {
    let mut out = String::new();
    emit_cpp_includes(&mut out, file);
    out.push('\n');
    emit_cpp_helpers(&mut out, file);
    let unit_fns = codegen_c::collect_unit_functions(file);
    for func in &file.bullets {
        if func.name == "main" {
            out.push_str(&emit_main_function_cpp(func, &unit_fns));
        } else {
            out.push_str(&emit_function_cpp(func, &unit_fns));
        }
        out.push('\n');
    }
    out
}

// ── Struct emitter ────────────────────────────────────────────────────────────

pub fn emit_struct_cpp(s: &bullang::ast::StructDef) -> String {
    let mut out = String::new();
    out.push_str(&format!("struct {} {{\n", s.name));
    for field in &s.fields {
        out.push_str(&format!("    {} {};\n", bu_type_to_cpp(&field.ty), field.name));
    }
    out.push_str("};\n");
    out
}

pub fn emit_enum_cpp(e: &bullang::ast::EnumDef) -> String {
    let mut out = String::new();
    out.push_str(&format!("enum class {} {{\n", e.name));
    for v in &e.variants {
        out.push_str(&format!("    {},\n", v.name));
    }
    out.push_str("};\n");
    out
}

// ── Header file ───────────────────────────────────────────────────────────────

pub fn emit_header_cpp(
    module_name:  &str,
    source_files: &[(String, &SourceFile)],
    namespace:    &str,
    includes:     &[String],
    structs:      &[bullang::ast::StructDef],
    enums:        &[bullang::ast::EnumDef],
    natives:      &[bullang::ast::NativeBlock],
) -> String {
    let guard = format!("{}_HPP", module_name.to_uppercase().replace('-', "_"));
    let mut out = String::new();

    out.push_str(&format!("#pragma once\n#ifndef {}\n#define {}\n\n", guard, guard));
    out.push_str("#include <cstdint>\n");
    out.push_str("#include <cstddef>\n");
    out.push_str("#include <string>\n");
    out.push_str("#include <vector>\n");
    out.push_str("#include <unordered_map>\n");
    out.push_str("#include <optional>\n");
    out.push_str("#include <functional>\n");
    out.push_str("#include <algorithm>\n");
    out.push_str("#include <numeric>\n");
    out.push_str("#include <cmath>\n");
    out.push_str("#include <cctype>\n");
    for inc in includes {
        out.push_str(&format!("#include <{}>\n", inc));
    }
    out.push('\n');

    out.push_str(&format!("namespace {} {{\n\n", namespace));

    // Enum class definitions — scoped, no global-namespace pollution
    for e in enums {
        out.push_str(&emit_enum_cpp(e));
        out.push('\n');
    }

    // Inventory struct definitions
    for s in structs {
        out.push_str(&emit_struct_cpp(s));
        out.push('\n');
    }

    // Verbatim native blocks (e.g. @cpp class definitions from inventory.bu)
    for nb in natives {
        if nb.backend == bullang::ast::Backend::Cpp {
            out.push_str("// @cpp native block\n");
            out.push_str(nb.code.trim());
            out.push_str("\n\n");
        } else {
            out.push_str(&format!(
                "// WARNING: @{} native block skipped in C++ header\n\n",
                nb.backend.escape_keyword()
            ));
        }
    }

    for (filename, sf) in source_files {
        out.push_str(&format!("// {}\n", filename));
        for func in &sf.bullets {
            let params = cpp_param_list(&func.params);
            let ret    = bu_type_to_cpp(&func.output.as_ref().map(|o| &o.ty).unwrap_or(&bullang::ast::BuType::unit()));
            out.push_str(&format!("{} {}({});\n", ret, func.name, params));
        }
        out.push('\n');
    }

    out.push_str(&format!("}} // namespace {}\n\n", namespace));
    out.push_str(&format!("#endif // {}\n", guard));
    out
}

// ── main.bu → main.cpp ───────────────────────────────────────────────────────

pub fn emit_main_cpp(file: &SourceFile, header_name: &str, namespace: &str) -> String {
    let mut out = String::new();
    out.push_str("#include <iostream>\n");
    emit_cpp_includes(&mut out, file);
    out.push_str(&format!("#include \"{}\"\n", header_name));
    out.push_str(&format!("using namespace {};\n\n", namespace));
    emit_cpp_helpers(&mut out, file);

    let unit_fns = codegen_c::collect_unit_functions(file);
    for func in &file.bullets {
        if func.name == "main" {
            out.push_str(&emit_main_function_cpp(func, &unit_fns));
        } else {
            out.push_str(&emit_function_cpp(func, &unit_fns));
        }
        out.push('\n');
    }
    out
}

pub fn emit_makefile_cpp(
    crate_name:   &str,
    source_files: &[String],
    has_main:     bool,
) -> String {
    let objects: Vec<String> = source_files.iter()
        .map(|f| f.replace(".cpp", ".o"))
        .collect();
    let obj_str = objects.join(" ");

    let mut out = String::new();
    out.push_str("CXX      = c++\n");
    out.push_str("CXXFLAGS = -Wall -Werror -Wextra -g -std=c++17\n");
    out.push_str(&format!("TARGET   = {}\n\n", crate_name));
    out.push_str(&format!("OBJECTS  = {}\n\n", obj_str));

    if has_main {
        out.push_str("all: $(TARGET)\n\n");
        out.push_str("$(TARGET): $(OBJECTS)\n");
        out.push_str("\t$(CXX) $(CXXFLAGS) -o $@ $^\n\n");
    } else {
        out.push_str(&format!("all: lib{}.a\n\n", crate_name));
        out.push_str(&format!("lib{}.a: $(OBJECTS)\n", crate_name));
        out.push_str("\tar rcs $@ $^\n\n");
    }

    out.push_str("%.o: %.cpp\n");
    out.push_str("\t$(CXX) $(CXXFLAGS) -c -o $@ $<\n\n");

    out.push_str("clean:\n");
    out.push_str(&format!("\trm -f $(OBJECTS) $(TARGET) lib{}.a\n\n", crate_name));
    out.push_str(".PHONY: all clean\n");
    out
}

// ── Function emitters ─────────────────────────────────────────────────────────

fn emit_function_cpp(func: &Bullet, unit_fns: &BTreeSet<&str>) -> String {
    let mut out   = String::new();
    let params    = cpp_param_list(&func.params);
    let ret       = bu_type_to_cpp(&func.output.as_ref().map(|o| &o.ty).unwrap_or(&bullang::ast::BuType::unit()));

    if !func.type_params.is_empty() {
        let tparams = func.type_params.iter()
            .map(|t| format!("typename {}", t))
            .collect::<Vec<_>>().join(", ");
        out.push_str(&format!("template<{}>\n", tparams));
    }

    out.push_str(&format!("{} {}({}) {{\n", ret, func.name, params));
    emit_body_cpp(&mut out, &func.body, &func.params, ret == "void", unit_fns);
    out.push_str("}\n");
    out
}

fn emit_main_function_cpp(func: &Bullet, unit_fns: &BTreeSet<&str>) -> String {
    let mut out = String::new();
    out.push_str("int main() {\n");
    emit_body_cpp(&mut out, &func.body, &func.params, true, unit_fns);
    out.push_str("    return 0;\n");
    out.push_str("}\n");
    out
}

// ── Expression emitters ───────────────────────────────────────────────────────
// C++ delegates most emission to the C backend, but patches EnumVariant:
// C emits bare names (global typedef enum), C++ needs `Direction::North`
// (scoped enum class).

fn emit_expr_cpp(expr: &Expr) -> String {
    match expr {
        Expr::Atom(a)      => emit_atom_cpp(a),
        Expr::BinOp(b)     => format!("{} {} {}",
            emit_atom_cpp(&b.lhs), b.op, emit_atom_cpp(&b.rhs)),
        Expr::Tuple(exprs) => format!(
            "({})", exprs.iter().map(emit_expr_cpp).collect::<Vec<_>>().join(", ")
        ),
    }
}

fn emit_atom_cpp(atom: &Atom) -> String {
    match atom {
        // C++ enum class: Direction::North (scoped)
        Atom::EnumVariant { ty, variant } => format!("{}::{}", ty, variant),
        // An inline `builtin::name(args)`. C++ had no arm for this at all and
        // fell through to the C emitter, which emitted C's spelling of the
        // builtin — `bu_to_upper` where C++ declares `bu_to_upper` taking a
        // std::string, and C's `ft_`-style destination form where C++ returns
        // by value.
        Atom::BuiltinExpr { name, args } =>
            match crate::pipe::inline_builtin(name, args, &Backend::Cpp, &emit_expr_cpp) {
                Ok(code) => code,
                Err(e)   => format!("/* ERROR: {e} */"),
            },
        // Everything else: delegate to the C emitter, whose expression
        // spelling C++ shares.
        other => codegen_c::emit_atom_c(other),
    }
}

fn emit_body_cpp(out: &mut String, body: &BulletBody, params: &[Param], returns_unit: bool, unit_fns: &BTreeSet<&str>) {
    emit_body_cpp_typed(out, body, params, returns_unit, unit_fns, &std::collections::HashMap::new())
}

fn emit_body_cpp_typed(out: &mut String, body: &BulletBody, params: &[Param], returns_unit: bool, unit_fns: &BTreeSet<&str>, fn_outputs: &std::collections::HashMap<String, BuType>) {
    match body {
        BulletBody::Pipes(pipes) => {
            if pipes.is_empty() { return; }
            let last = pipes.len().saturating_sub(1);
            let env = crate::codegen::typeinfer::TypeEnv::seed(params, fn_outputs);
            for (i, pipe) in pipes.iter().enumerate() {
                let mut callee_is_unit = false;
                // Handle builtin::name with implicit pipe inputs
                let expr_str = if let Expr::Atom(Atom::BuiltinNoArgs(name)) = &pipe.expr {
                    let mut synthetic_params: Vec<bullang::ast::Param> = Vec::new();
                    for (idx, input) in pipe.inputs.iter().enumerate() {
                        let param_name = match input {
                            Expr::Atom(Atom::Ident(s)) => s.clone(),
                            _ => {
                                // Not a plain variable — declare a real
                                // temporary above the call and reference
                                // that instead (see codegen_c::emit_body_c).
                                let tmp = format!("__arg_{}", idx);
                                out.push_str(&format!(
                                    "    auto {} = {};\n",
                                    tmp, emit_expr_cpp(input)
                                ));
                                tmp
                            }
                        };
                        synthetic_params.push(bullang::ast::Param {
                            name: param_name,
                            ty:   env.infer(input),
                        });
                    }
                    match crate::stdlib::emit_builtin(name, &synthetic_params, &Backend::Cpp) {
                        Ok(code) => code,
                        Err(e)   => format!("/* ERROR: {e} */"),
                    }
                } else {
                    // See pipe.rs: the inputs are arguments only when the
                    // expression is a bare callee.
                    match crate::pipe::classify(pipe) {
                        crate::pipe::PipeRhs::Call { name, args } => {
                            callee_is_unit = unit_fns.contains(name);
                            format!(
                                "{}({})",
                                name,
                                args.iter().map(emit_expr_cpp).collect::<Vec<_>>().join(", ")
                            )
                        }
                        crate::pipe::PipeRhs::Expr(expr) => {
                            if let Expr::Atom(Atom::Call { name, .. }) = expr {
                                callee_is_unit = unit_fns.contains(name.as_str());
                            }
                            emit_expr_cpp(expr)
                        }
                        crate::pipe::PipeRhs::Builtin { .. } => unreachable!(),
                    }
                };
                // See codegen_c::emit_body_c for why unit-returning functions
                // (including main, whose C++ signature is forced to `int`
                // regardless of its Bullang-level `()` return type) never
                // turn their last pipe into `return expr;`.
                if i == last && !returns_unit {
                    out.push_str(&format!("    return {};\n", expr_str));
                } else if callee_is_unit || pipe.binding.is_none() {
                    // See codegen_c::emit_body_c — void callee, or an
                    // explicit `-> {}` discard; either way, a bare
                    // statement, relying on the builtin/function not
                    // wrapping its result in an explicit cast.
                    out.push_str(&format!("    {};\n", expr_str));
                } else {
                    let binding = pipe.binding.as_deref().unwrap();
                    out.push_str(&format!("    auto {} = {};\n", binding, expr_str));
                }
            }
        }
        BulletBody::Natives(blocks) => {
            let block = blocks.iter()
                .find(|b| b.backend == Backend::Cpp || b.backend == Backend::C);
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
                            "    /* ERROR: '@{}' cannot be used in a C++ build — use '@cpp' */\n",
                            b.backend.escape_keyword()
                        ));
                    }
                }
            }
        }
        BulletBody::Builtin(name) => {
            match crate::stdlib::emit_builtin(name, params, &Backend::Cpp) {
                // Same reasoning as the `i == last && !returns_unit` branch
                // above: a unit-returning function (`-> ()`, including
                // `main`) never turns its sole builtin call into
                // `return expr;`.
                Ok(code) if returns_unit => out.push_str(&format!("    {};\n", code)),
                Ok(code)                 => out.push_str(&format!("    return {};\n", code)),
                Err(e)                   => out.push_str(&format!("    // ERROR: {}\n", e)),
            }
        }
    }
}

// ── Type mapping: Bullang → C++ ───────────────────────────────────────────────

pub fn bu_type_to_cpp(ty: &BuType) -> String {
    match ty {
        BuType::Named(s)     => rust_type_to_cpp(s),
        BuType::Tuple(inner) => format!(
            "std::tuple<{}>",
            inner.iter().map(bu_type_to_cpp).collect::<Vec<_>>().join(", ")
        ),
        BuType::Unknown      => "auto".to_string(),
    }
}

fn rust_type_to_cpp(s: &str) -> String {
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
        "String" => "std::string".to_string(),
        "&str"   => "std::string_view".to_string(),
        "()"     => "void".to_string(),
        other    => translate_cpp_generic(other),
    }
}

fn translate_cpp_generic(s: &str) -> String {
    if s.starts_with("Vec[") && s.ends_with(']') {
        let inner = &s[4..s.len()-1];
        return format!("std::vector<{}>", rust_type_to_cpp(inner));
    }
    if s.starts_with("HashMap[") && s.ends_with(']') {
        let inner = &s[8..s.len()-1];
        let parts: Vec<&str> = inner.splitn(2, ',').collect();
        if parts.len() == 2 {
            return format!("std::unordered_map<{}, {}>",
                rust_type_to_cpp(parts[0].trim()),
                rust_type_to_cpp(parts[1].trim()));
        }
    }
    if s.starts_with("Option[") && s.ends_with(']') {
        let inner = &s[7..s.len()-1];
        return format!("std::optional<{}>", rust_type_to_cpp(inner));
    }
    if s.starts_with("&mut") {
        let inner = s[4..].trim();
        return format!("{}&", rust_type_to_cpp(inner));
    }
    if s.starts_with('&') {
        let inner = s[1..].trim();
        return format!("const {}&", rust_type_to_cpp(inner));
    }
    if s.starts_with("Fn[") {
        return translate_fn_type_cpp(s);
    }
    if s.starts_with('(') && s.contains(',') {
        // Tuple literal
        let inner = &s[1..s.len()-1];
        let parts: Vec<String> = inner.split(',')
            .map(|p| rust_type_to_cpp(p.trim()))
            .collect();
        return format!("std::tuple<{}>", parts.join(", "));
    }
    format!("{} /* ? */", s)
}

fn translate_fn_type_cpp(s: &str) -> String {
    // Fn[T, U -> V]  →  std::function<V(T, U)>
    let inner = s.trim_start_matches("Fn[").trim_end_matches(']');
    if inner.is_empty() { return "std::function<void()>".to_string(); }
    if let Some(arrow) = inner.find("->") {
        let params_str = inner[..arrow].trim();
        let ret_str    = inner[arrow+2..].trim();
        let params: Vec<String> = if params_str.is_empty() { vec![] }
            else { params_str.split(',').map(|p| rust_type_to_cpp(p.trim())).collect() };
        let ret = if ret_str.is_empty() { "void".to_string() }
            else { rust_type_to_cpp(ret_str) };
        format!("std::function<{}({})>", ret, params.join(", "))
    } else {
        let ret = rust_type_to_cpp(inner.trim());
        format!("std::function<{}()>", ret)
    }
}

fn cpp_param_list(params: &[Param]) -> String {
    if params.is_empty() { return String::new(); }
    params.iter()
        .map(|p| format!("{} {}", bu_type_to_cpp(&p.ty), p.name))
        .collect::<Vec<_>>().join(", ")
}
