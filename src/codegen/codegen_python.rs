//! Python code generation backend.
//!
//! Translates Bullang AST → Python source files.
//! Each bullet becomes a Python function.
//! All functions are module-level — Python has no `pub` keyword.

use crate::ast::*;

// ── Source file → Python ──────────────────────────────────────────────────────

pub fn emit_source_py(file: &SourceFile) -> String {
    let mut out = String::new();
    out.push_str("from __future__ import annotations\n");
    out.push_str("from typing import Any, Callable, Optional, List, Tuple, Dict\n\n");
    for func in &file.bullets {
        out.push_str(&emit_function_py(func));
        out.push('\n');
    }
    out
}

// ── main.bu → __main__.py ─────────────────────────────────────────────────────

pub fn emit_main_py(file: &SourceFile, _module_name: &str) -> String {
    let mut out = String::new();
    out.push_str("from __future__ import annotations\n");
    out.push_str("from . import *\n\n");
    for func in &file.bullets {
        if func.name == "main" {
            out.push_str(&emit_main_function_py(func));
        } else {
            out.push_str(&emit_function_py(func));
        }
        out.push('\n');
    }
    out.push_str("if __name__ == \"__main__\":\n");
    out.push_str("    main()\n");
    out
}

// ── Module init file ──────────────────────────────────────────────────────────

pub fn emit_init_py(child_modules: &[String], structs: &[crate::ast::StructDef], enums: &[crate::ast::EnumDef]) -> String {
    let mut out = String::new();
    out.push_str("from __future__ import annotations\n");
    out.push_str("from typing import Any, Callable, Optional, List, Tuple, Dict\n");
    if !structs.is_empty() {
        out.push_str("from dataclasses import dataclass\n");
    }
    if !enums.is_empty() {
        out.push_str("from enum import Enum\n");
    }
    out.push('\n');
    for s in structs {
        out.push_str(&emit_struct_py(s));
        out.push('\n');
    }
    for e in enums {
        out.push_str(&emit_enum_py(e));
        out.push('\n');
    }
    for module in child_modules {
        out.push_str(&format!("from .{} import *\n", module));
    }
    out
}

// ── Struct emitter ────────────────────────────────────────────────────────────

pub fn emit_struct_py(s: &crate::ast::StructDef) -> String {
    let mut out = String::new();
    out.push_str("@dataclass\n");
    out.push_str(&format!("class {}:\n", s.name));
    if s.fields.is_empty() {
        out.push_str("    pass\n");
    } else {
        for field in &s.fields {
            out.push_str(&format!("    {}: {}\n", field.name, bu_type_to_python(&field.ty)));
        }
    }
    out
}

// ── Enum emitter ──────────────────────────────────────────────────────────────

pub fn emit_enum_py(e: &crate::ast::EnumDef) -> String {
    let mut out = String::new();
    out.push_str(&format!("class {}(Enum):\n", e.name));
    if e.variants.is_empty() {
        out.push_str("    pass\n");
    } else {
        for (i, v) in e.variants.iter().enumerate() {
            out.push_str(&format!("    {} = {}\n", v.name, i));
        }
    }
    out
}

// ── Function emitters ─────────────────────────────────────────────────────────

fn py_param_name(name: &str) -> &str {
    // Python reserved words that may appear as Bullang param names
    match name {
        "from" => "from_",
        "import" => "import_",
        "class" => "class_",
        "return" => "return_",
        "pass" => "pass_",
        "for" => "for_",
        "while" => "while_",
        "in" => "in_",
        "not" => "not_",
        "and" => "and_",
        "or" => "or_",
        "if" => "if_",
        "else" => "else_",
        "lambda" => "lambda_",
        "with" => "with_",
        "as" => "as_",
        "try" => "try_",
        "except" => "except_",
        "raise" => "raise_",
        "del" => "del_",
        other => other,
    }
}

fn emit_function_py(func: &Bullet) -> String {
    let mut out = String::new();

    // Emit TypeVar declarations for each type param
    for tp in &func.type_params {
        out.push_str(&format!("{} = TypeVar('{}')\n", tp, tp));
    }

    let params = func.params.iter()
        .map(|p| format!("{}: {}", py_param_name(&p.name), bu_type_to_python(&p.ty)))
        .collect::<Vec<_>>().join(", ");

    let ret_ty = bu_type_to_python(&func.output.ty);
    out.push_str(&format!("def {}({}) -> {}:\n", func.name, params, ret_ty));

    emit_body_py(&mut out, &func.body, &func.params);
    out
}

fn emit_main_function_py(func: &Bullet) -> String {
    let mut out = String::new();
    out.push_str("def main() -> None:\n");
    emit_body_py(&mut out, &func.body, &func.params);
    out
}

fn emit_body_py(out: &mut String, body: &BulletBody, params: &[Param]) {
    match body {
        BulletBody::Pipes(pipes) => {
            if pipes.is_empty() {
                out.push_str("    pass\n");
                return;
            }
            let last = pipes.len().saturating_sub(1);
            for (i, pipe) in pipes.iter().enumerate() {
                let expr_str = emit_expr_py(&pipe.expr);
                out.push_str(&format!("    {} = {}\n", pipe.binding, expr_str));
                if pipe.propagate {
                    // Option → return None if falsy; Result → return the error value
                    out.push_str(&format!("    if {} is None: return None\n", pipe.binding));
                }
                if i == last {
                    out.push_str(&format!("    return {}\n", pipe.binding));
                }
            }
        }
        BulletBody::Natives(blocks) => {
            let block = blocks.iter().find(|b| b.backend == Backend::Python);
            match block {
                Some(b) => {
                    let base_indent = b.code.lines()
                        .filter(|l| !l.trim().is_empty())
                        .map(|l| l.len() - l.trim_start().len())
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
                    // Try to find any block — fall back to error
                    if let Some(b) = blocks.first() {
                        out.push_str(&format!(
                            "    raise NotImplementedError(\"'@{}' block cannot run in Python — use '@python' instead\")\n",
                            b.backend.escape_keyword()
                        ));
                    }
                }
            }
        }
        BulletBody::Builtin(name) => {
            use crate::stdlib;
            match stdlib::emit_builtin(name, params, &Backend::Python) {
                Ok(code) => out.push_str(&format!("    return {}\n", code)),
                Err(e)   => out.push_str(&format!("    raise NotImplementedError(\"{}\")\n", e)),
            }
        }
    }
}

// ── Expression emitters ───────────────────────────────────────────────────────

fn emit_expr_py(expr: &Expr) -> String {
    match expr {
        Expr::Atom(a)      => emit_atom_py(a),
        Expr::BinOp(b)     => {
            // Python uses keyword operators instead of symbols
            let op = match b.op.as_str() {
                "&&" => "and",
                "||" => "or",
                other => other,
            };
            format!("{} {} {}", emit_atom_py(&b.lhs), op, emit_atom_py(&b.rhs))
        }
        Expr::Tuple(exprs) => format!(
            "({})", exprs.iter().map(emit_expr_py).collect::<Vec<_>>().join(", ")
        ),
    }
}

fn emit_atom_py(atom: &Atom) -> String {
    match atom {
        Atom::Ident(s)         => s.clone(),
        Atom::Float(n) => n.to_string(),
        Atom::Integer(n)       => n.to_string(),
        Atom::StringLit(s)     => format!("\"{}\"", s),
        Atom::Interp(template) => format!("f\"{}\"", template),
        Atom::Call { name, args } => {
            let args_str = args.iter().map(|a| match a {
                CallArg::Value(s)     => s.clone(),
                CallArg::BulletRef(s) => s.clone(),
            }).collect::<Vec<_>>().join(", ");
            format!("{}({})", name, args_str)
        }
        Atom::BuiltinExpr { name, args } => {
            match name.as_str() {
                "assert" => {
                    let cond = emit_expr_py(&args[0]);
                    format!(
                        "(lambda __r: __r if __r else \
                         [__import__('sys').stderr.write('[assert] failed\\n'), __r][-1])({})",
                        cond
                    )
                }
                "assert_eq" => {
                    let lhs = emit_expr_py(&args[0]);
                    let rhs = emit_expr_py(&args[1]);
                    format!(
                        "(lambda __l, __r: __l == __r if __l == __r else \
                         [__import__('sys').stderr.write(\
                           f'[assert_eq] expected {{__r!r}}, got {{__l!r}}\\n'\
                         ), False][-1])({lhs}, {rhs})"
                    )
                }
                "assert_ne" => {
                    let lhs = emit_expr_py(&args[0]);
                    let rhs = emit_expr_py(&args[1]);
                    format!(
                        "(lambda __l, __r: __l != __r if __l != __r else \
                         [__import__('sys').stderr.write(\
                           f'[assert_ne] expected values to differ, both were {{__l!r}}\\n'\
                         ), False][-1])({lhs}, {rhs})"
                    )
                }
                other => format!("None  # builtin::{other} not supported as expression"),
            }
        }
        Atom::Unary { op, rhs } => {
            // Python uses `not` for boolean negation; `-` is the same
            let py_op = if op == "!" { "not " } else { op.as_str() };
            format!("({}{})", py_op, emit_atom_py(rhs))
        }
        Atom::FieldAccess { base, fields } => format!("{}.{}", base, fields.join(".")),
        Atom::Index { base, idx } =>
            format!("{}[{}]", base, emit_expr_py(idx)),
        Atom::Slice { base, from, to } =>
            format!("{}[{}:{}]", base, emit_expr_py(from), emit_expr_py(to)),
        Atom::EnumVariant { ty, variant } => format!("{}.{}", ty, variant),
        Atom::Closure { params, body, .. } => {
            let ps = params.iter()
                .map(|p| py_param_name(&p.name).to_string())
                .collect::<Vec<_>>().join(", ");
            format!("lambda {}: {}", ps, emit_expr_py(body))
        }
    }
}

// ── Type mapping: Bullang → Python type hints ─────────────────────────────────

pub fn bu_type_to_python(ty: &BuType) -> String {
    match ty {
        BuType::Named(s) => rust_type_to_python(s),
        BuType::Tuple(inner) => format!(
            "Tuple[{}]",
            inner.iter().map(bu_type_to_python).collect::<Vec<_>>().join(", ")
        ),
        BuType::Array(elem, n) => format!("List[{}]  # [T; {}]", bu_type_to_python(elem), n),
        BuType::Unknown        => "Any".to_string(),
    }
}

fn rust_type_to_python(s: &str) -> String {
    // Strip whitespace for normalised matching
    let s = s.split_whitespace().collect::<String>();
    match s.as_str() {
        // Integer types → int
        "i8"|"i16"|"i32"|"i64"|"i128"|"isize"
        |"u8"|"u16"|"u32"|"u64"|"u128"|"usize" => "int".to_string(),
        // Float types → float
        "f32"|"f64" => "float".to_string(),
        // Boolean
        "bool" => "bool".to_string(),
        // String types
        "String"|"&str"|"&\'staticstr" => "str".to_string(),
        // Unit type
        "()" => "None".to_string(),
        // Passthrough with best-effort translation for generics
        other => translate_generic_type(other),
    }
}

fn translate_generic_type(s: &str) -> String {
    if s.starts_with("Vec[") && s.ends_with(']') {
        let inner = &s[4..s.len()-1];
        return format!("List[{}]", rust_type_to_python(inner));
    }
    if s.starts_with("Option[") && s.ends_with(']') {
        let inner = &s[7..s.len()-1];
        return format!("Optional[{}]", rust_type_to_python(inner));
    }
    if s.starts_with("Result<") && s.ends_with('>') {
        // Simplified: just use the Ok type
        let inner = s[7..s.len()-1].split(',').next().unwrap_or("Any");
        return format!("Optional[{}]  # Result type", rust_type_to_python(inner.trim()));
    }
    if s.starts_with("Fn[") {
        // fn(T) -> U → Callable[[T], U]
        return translate_fn_type(s);
    }
    if s.starts_with('(') && s.contains(',') {
        // Tuple literal type
        let inner = &s[1..s.len()-1];
        let parts: Vec<String> = inner.split(',')
            .map(|p| rust_type_to_python(p.trim()))
            .collect();
        return format!("Tuple[{}]", parts.join(", "));
    }
    // Unknown: pass through as-is with a comment
    format!("Any  # {}", s)
}

fn translate_fn_type(s: &str) -> String {
    // Parse Fn[T, U -> V] → Callable[[T, U], V]
    let inner = s.trim_start_matches("Fn[").trim_end_matches(']');
    if inner.is_empty() { return "Callable".to_string(); }
    if let Some(arrow) = inner.find("->") {
        let params_str = inner[..arrow].trim();
        let ret_str    = inner[arrow+2..].trim();
        let params: Vec<String> = if params_str.is_empty() { vec![] }
            else { params_str.split(',').map(|p| rust_type_to_python(p.trim())).collect() };
        let ret = if ret_str.is_empty() { "None".to_string() }
            else { rust_type_to_python(ret_str) };
        format!("Callable[[{}], {}]", params.join(", "), ret)
    } else {
        let ret = rust_type_to_python(inner.trim());
        format!("Callable[[], {}]", ret)
    }
}
