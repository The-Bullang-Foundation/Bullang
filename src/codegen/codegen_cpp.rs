//! C++ code generation backend.
//!
//! Produces a C++ source file (.cpp) per Bullang source file,
//! a shared header (.hpp) with declarations inside a namespace,
//! and a Makefile.

use crate::ast::*;
use crate::codegen::codegen_c;

// ── Source file → C++ ────────────────────────────────────────────────────────

pub fn emit_source_cpp(file: &SourceFile, header_name: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("#include \"{}\"\n", header_name));
    out.push_str("#include <cstdlib>\n");
    out.push_str("#include <cstring>\n\n");

    let ns = header_name.trim_end_matches(".hpp");
    out.push_str(&format!("namespace {} {{\n\n", ns));
    for func in &file.bullets {
        out.push_str(&emit_function_cpp(func));
        out.push('\n');
    }
    out.push_str(&format!("}} // namespace {}\n", ns));
    out
}

// ── Struct emitter ────────────────────────────────────────────────────────────

pub fn emit_struct_cpp(s: &crate::ast::StructDef) -> String {
    let mut out = String::new();
    out.push_str(&format!("struct {} {{\n", s.name));
    for field in &s.fields {
        out.push_str(&format!("    {} {};\n", bu_type_to_cpp(&field.ty), field.name));
    }
    out.push_str("};\n");
    out
}

pub fn emit_enum_cpp(e: &crate::ast::EnumDef) -> String {
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
    structs:      &[crate::ast::StructDef],
    enums:        &[crate::ast::EnumDef],
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

    for (filename, sf) in source_files {
        out.push_str(&format!("// {}\n", filename));
        for func in &sf.bullets {
            let params = cpp_param_list(&func.params);
            let ret    = bu_type_to_cpp(&func.output.ty);
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
    out.push_str(&format!("#include \"{}\"\n", header_name));
    out.push_str(&format!("using namespace {};\n\n", namespace));

    for func in &file.bullets {
        if func.name == "main" {
            out.push_str(&emit_main_function_cpp(func));
        } else {
            out.push_str(&emit_function_cpp(func));
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

fn emit_function_cpp(func: &Bullet) -> String {
    let mut out   = String::new();
    let params    = cpp_param_list(&func.params);
    let ret       = bu_type_to_cpp(&func.output.ty);

    if !func.type_params.is_empty() {
        let tparams = func.type_params.iter()
            .map(|t| format!("typename {}", t))
            .collect::<Vec<_>>().join(", ");
        out.push_str(&format!("template<{}>\n", tparams));
    }

    out.push_str(&format!("{} {}({}) {{\n", ret, func.name, params));
    emit_body_cpp(&mut out, &func.body, &func.params);
    out.push_str("}\n");
    out
}

fn emit_main_function_cpp(func: &Bullet) -> String {
    let mut out = String::new();
    out.push_str("int main() {\n");
    emit_body_cpp(&mut out, &func.body, &func.params);
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
        // C++ closure: immediately-returned lambda
        Atom::Closure { params, ret, body } => {
            let ps = params.iter()
                .map(|p| format!("{} {}", bu_type_to_cpp(&p.ty), p.name))
                .collect::<Vec<_>>().join(", ");
            let ret_str  = bu_type_to_cpp(ret);
            let body_str = emit_expr_cpp(body);
            format!("[&]({}) -> {} {{ return {}; }}", ps, ret_str, body_str)
        }
        // Everything else: delegate to C emitter
        other => codegen_c::emit_atom_c(other),
    }
}

fn emit_body_cpp(out: &mut String, body: &BulletBody, params: &[Param]) {
    match body {
        BulletBody::Pipes(pipes) => {
            if pipes.is_empty() { return; }
            let last = pipes.len().saturating_sub(1);
            for (i, pipe) in pipes.iter().enumerate() {
                let expr_str = emit_expr_cpp(&pipe.expr);
                if i == last {
                    out.push_str(&format!("    return {};\n", expr_str));
                } else {
                    out.push_str(&format!("    auto {} = {};\n", pipe.binding, expr_str));
                    if pipe.propagate {
                        // C++ std::optional — if nullopt, return nullopt
                        out.push_str(&format!(
                            "    if (!{}) {{ return std::nullopt; }}\n",
                            pipe.binding
                        ));
                    }
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
            use crate::stdlib;
            match stdlib::emit_builtin(name, params, &Backend::Cpp) {
                Ok(code) => out.push_str(&format!("    return {};\n", code)),
                Err(e)   => out.push_str(&format!("    // ERROR: {}\n", e)),
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
        BuType::Array(t, n)  => format!("std::array<{}, {}>", bu_type_to_cpp(t), n),
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
