//! Python code generation backend.
//!
//! Translates Bullang AST → Python source files.
//! Each bullet becomes a Python function.
//! All functions are module-level — Python has no `pub` keyword.

use bullang::ast::*;

// ── Hoisted imports and helpers ─────────────────────────────────────────────

/// Python had no collection step at all: its preamble was a fixed list, so a
/// builtin needing anything beyond it emitted a call to a function that was
/// never defined. Helpers are module-level `def`s, which is also what let the
/// `lambda`/`__import__` one-liners the old builtins used go away.
/// `TypeVar` declarations for every type parameter in the file.
///
/// These were emitted immediately above each generic function, so two generic
/// functions over `T` produced two `T = TypeVar('T')` lines — a rebinding
/// Python accepts but type checkers flag, and which reads as though the two
/// `T`s were unrelated. Hoisted to module level and deduplicated.
fn emit_py_typevars(out: &mut String, file: &SourceFile) {
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for func in &file.bullets {
        for tp in &func.type_params {
            seen.insert(tp.as_str());
        }
    }
    if seen.is_empty() {
        return;
    }
    out.push_str("from typing import TypeVar\n");
    for tp in &seen {
        out.push_str(&format!("{} = TypeVar('{}')\n", tp, tp));
    }
    out.push('\n');
}

fn emit_py_preamble(out: &mut String, file: &SourceFile) {
    emit_py_typevars(out, file);
    let hoisted = crate::codegen::hoist::requirements(file, &Backend::Python);
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

// ── Source file → Python ──────────────────────────────────────────────────────

pub fn emit_source_py(file: &SourceFile) -> String {
    let mut out = String::new();
    out.push_str("from __future__ import annotations\n");
    out.push_str("from typing import Any, Callable, Optional, List, Tuple, Dict\n\n");
    emit_py_preamble(&mut out, file);
    for func in &file.bullets {
        out.push_str(&emit_function_py(func));
        out.push('\n');
    }
    out
}

/// Bare single-file mode: only the function bodies, no imports, no preamble.
pub fn emit_bare_py(file: &SourceFile) -> String {
    let mut out = String::new();
    emit_py_preamble(&mut out, file);
    for func in &file.bullets {
        out.push_str(&emit_function_py(func));
        out.push('\n');
    }
    out
}

// ── main.bu → __main__.py ─────────────────────────────────────────────────────

pub fn emit_main_py(file: &SourceFile, module_name: &str) -> String {
    let mut out = String::new();
    out.push_str("from __future__ import annotations\n");
    // Absolute, not `from . import *`: a relative import needs the package
    // context, so running the file directly — `python __main__.py`, which is
    // what someone naturally tries — failed with "attempted relative import
    // with no known parent package".
    out.push_str(&format!("from {} import *\n\n", module_name));
    emit_py_preamble(&mut out, file);
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

pub fn emit_init_py(child_modules: &[String], structs: &[bullang::ast::StructDef], enums: &[bullang::ast::EnumDef]) -> String {
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

pub fn emit_struct_py(s: &bullang::ast::StructDef) -> String {
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

pub fn emit_enum_py(e: &bullang::ast::EnumDef) -> String {
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

    let params = func.params.iter()
        .map(|p| format!("{}: {}", py_param_name(&p.name), bu_type_to_python(&p.ty)))
        .collect::<Vec<_>>().join(", ");

    let ret_ty = bu_type_to_python(&func.output.as_ref().map(|o| &o.ty).unwrap_or(&bullang::ast::BuType::unit()));
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
    emit_body_py_typed(out, body, params, &std::collections::HashMap::new())
}

fn emit_body_py_typed(out: &mut String, body: &BulletBody, params: &[Param], fn_outputs: &std::collections::HashMap<String, BuType>) {
    match body {
        BulletBody::Pipes(pipes) => {
            if pipes.is_empty() {
                out.push_str("    pass\n");
                return;
            }
            let last = pipes.len().saturating_sub(1);
            let env = crate::codegen::typeinfer::TypeEnv::seed(params, fn_outputs);
            // Counter has to be unique across the whole function body, not
            // reset per pipe — see the matching comment in emit_body_go,
            // where reusing a name is a hard compile error. Python doesn't
            // require this (plain `=` tolerates reassignment) but keeping
            // the same convention avoids confusing reused temp names in
            // generated output across pipes.
            let mut tmp_counter: usize = 0;
            for (i, pipe) in pipes.iter().enumerate() {
                // Handle builtin::name with implicit pipe inputs
                let expr_str = if let Expr::Atom(Atom::BuiltinNoArgs(name)) = &pipe.expr {
                    let synthetic_params: Vec<bullang::ast::Param> = pipe.inputs
                        .iter()
                        .map(|input| {
                            let param_name = match input {
                                Expr::Atom(Atom::Ident(s)) => s.clone(),
                                // Not a plain variable — declare a real
                                // temporary above the call and reference
                                // that instead. Previously this fell back to
                                // a made-up `__pipe_arg_N` name with no
                                // matching declaration anywhere, so any
                                // multi-arg implicit call with a non-ident
                                // input (e.g. `(path, "w"): builtin::open`)
                                // produced Python that referenced an
                                // undefined name — see codegen_c.rs's
                                // equivalent block, which already does this
                                // correctly.
                                _ => {
                                    let tmp = format!("__arg_{}", tmp_counter);
                                    tmp_counter += 1;
                                    out.push_str(&format!(
                                        "    {} = {}\n",
                                        tmp, emit_expr_py(input)
                                    ));
                                    tmp
                                }
                            };
                            bullang::ast::Param {
                                name: param_name,
                                ty:   env.infer(input),
                            }
                        })
                        .collect();
                    match crate::stdlib::emit_builtin(name, &synthetic_params, &Backend::Python) {
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
                            name.to_string(),
                            args.iter().map(emit_expr_py).collect::<Vec<_>>().join(", ")
                        ),
                        crate::pipe::PipeRhs::Expr(expr) => emit_expr_py(expr),
                        // Handled above, before this branch is reached.
                        crate::pipe::PipeRhs::Builtin { .. } => unreachable!(),
                    }
                };
                out.push_str(&format!("    {} = {}\n", pipe.binding.as_deref().unwrap_or("_"), expr_str));
                if i == last {
                    out.push_str(&format!("    return {}\n", pipe.binding.as_deref().unwrap_or("_")));
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
                CallArg::Value(s) => s.clone(),
            }).collect::<Vec<_>>().join(", ");
            format!("{}({})", name, args_str)
        }
        Atom::BuiltinNoArgs(name) => format!(
            "/* ERROR: 'builtin::{name}' needs its arguments — give it its own \
             bullet, or call it as 'builtin::{name}(args)' */"
        ),
        Atom::BuiltinExpr { name, args } =>
            match crate::pipe::inline_builtin(name, args, &Backend::Python, &emit_expr_py) {
                Ok(code) => code,
                Err(e)   => format!("/* ERROR: {e} */"),
            },
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
    // A name Python has no equivalent for — a type parameter, or a
    // user-declared struct or enum — is emitted as itself.
    //
    // It used to come out as `Any  # T`, which is a comment: in
    // `def pick(a: Any  # T, b: Any  # T) -> Any  # T:` everything from the
    // first `#` onward is commented out, including the closing paren and the
    // colon, so the file would not even parse. A bare name is both valid and
    // more informative — `T` resolves to the module-level `TypeVar`, and a
    // struct name resolves to its dataclass.
    s.to_string()
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
