//! Java code generation backend.
//!
//! Each Bullang source file becomes a Java class whose name matches the file
//! stem (snake_case → PascalCase).  All bullets become `public static` methods
//! so callers never need to instantiate the class.
//!
//! When main.bu is present it produces `Main.java` with a standard
//! `public static void main(String[] args)` entry point.
//!
//! Type mapping:
//!   i8/i16/i32/i64 → byte/short/int/long
//!   u8/u16/u32/u64 → int/int/long/long   (Java has no unsigned — widened)
//!   usize/isize    → long/long
//!   f32/f64        → float/double
//!   bool           → boolean
//!   char           → char
//!   String/&str    → String
//!   Vec[T]         → java.util.ArrayList<T>
//!   HashMap[K,V]   → java.util.HashMap<K,V>
//!   Option[T]      → T  (nullable — no wrapper type)
//!   Tuple[T,U]     → long[] / Object[]  (via named inner class)
//!   ()             → void
//!   fn(T)->U       → java.util.function.Function<T,U>

use bullang::ast::*;

// ── Hoisted imports and helpers ─────────────────────────────────────────────

/// Java, like Python, had a fixed preamble and no collection step.
///
/// Helpers here are class members, not top-level declarations — Java has no
/// free functions — so they are emitted inside the generated class body,
/// indented once. That is also why they replaced the `IntSupplier` and
/// `Supplier<String>` lambdas the old builtins wrapped everything in: a
/// static method is what a reader expects to find, and it can declare
/// `throws` handling normally.
fn emit_java_imports(out: &mut String, file: &SourceFile) {
    for import in crate::codegen::hoist::requirements(file, &Backend::Java).imports {
        out.push_str(import);
        out.push('\n');
    }
}

fn emit_java_helpers(out: &mut String, file: &SourceFile) {
    for helper in crate::codegen::hoist::requirements(file, &Backend::Java).helpers {
        for line in helper.lines() {
            if line.trim().is_empty() {
                out.push('\n');
            } else {
                out.push_str("    ");
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push('\n');
    }
}
use crate::codegen::to_pascal_case;

// ── Source file → Java ────────────────────────────────────────────────────────

/// Full class file — used when building a project.
pub fn emit_source_java(file: &SourceFile, class_name: &str) -> String {
    let mut out = String::new();
    emit_java_imports(&mut out, file);
    out.push('\n');
    out.push_str(&format!("public class {} {{\n\n", class_name));
    emit_java_helpers(&mut out, file);
    for func in &file.bullets {
        out.push_str(&emit_function_java(func));
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

/// Bare single-file mode: only the method bodies, no class wrapper.
pub fn emit_bare_java(file: &SourceFile) -> String {
    let mut out = String::new();
    emit_java_helpers(&mut out, file);
    for func in &file.bullets {
        out.push_str(&emit_function_java(func));
        out.push('\n');
    }
    out
}

// ── main.bu → Main.java ───────────────────────────────────────────────────────

pub fn emit_main_java(file: &SourceFile, _crate_name: &str) -> String {
    let mut out = String::new();
    out.push_str("import java.util.*;\n");
    out.push_str("import java.util.function.*;\n");
    emit_java_imports(&mut out, file);
    out.push('\n');
    out.push_str("public class Main {\n\n");
    emit_java_helpers(&mut out, file);
    for func in &file.bullets {
        if func.name == "main" {
            out.push_str(&emit_main_function_java(func));
        } else {
            out.push_str(&emit_function_java(func));
        }
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

// ── Struct / enum emitters ────────────────────────────────────────────────────

pub fn emit_struct_java(s: &StructDef) -> String {
    let mut out = String::new();
    let name = to_pascal_case(&s.name);
    out.push_str(&format!("    public static class {} {{\n", name));
    for field in &s.fields {
        out.push_str(&format!(
            "        public {} {};\n",
            bu_type_to_java(&field.ty),
            field.name
        ));
    }
    // Constructor
    let params: Vec<String> = s.fields.iter()
        .map(|f| format!("{} {}", bu_type_to_java(&f.ty), f.name))
        .collect();
    out.push_str(&format!("        public {}({}) {{\n", name, params.join(", ")));
    for field in &s.fields {
        out.push_str(&format!("            this.{f} = {f};\n", f = field.name));
    }
    out.push_str("        }\n");
    out.push_str("    }\n");
    out
}

pub fn emit_enum_java(e: &EnumDef) -> String {
    let mut out = String::new();
    out.push_str(&format!("    public enum {} {{\n        ", e.name));
    let variants: Vec<&str> = e.variants.iter().map(|v| v.name.as_str()).collect();
    out.push_str(&variants.join(", "));
    out.push_str("\n    }\n");
    out
}

/// Top-level types file — emitted as a class containing all struct/enum inner classes.
pub fn emit_types_java(
    class_name: &str,
    structs: &[StructDef],
    enums: &[EnumDef],
) -> String {
    let mut out = String::new();
    out.push_str(&format!("public class {} {{\n\n", class_name));
    for e in enums {
        out.push_str(&emit_enum_java(e));
        out.push('\n');
    }
    for s in structs {
        out.push_str(&emit_struct_java(s));
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

// ── Function emitters ─────────────────────────────────────────────────────────

fn emit_function_java(func: &Bullet) -> String {
    let mut out = String::new();
    let ret = bu_type_to_java(
        &func.output.as_ref()
            .map(|o| o).unwrap_or_else(|| panic!("no output"))
            .ty,
    );
    let params: String = func.params.iter()
        .map(|p| format!("{} {}", bu_type_to_java(&p.ty), p.name))
        .collect::<Vec<_>>()
        .join(", ");

    let type_params = if func.type_params.is_empty() {
        String::new()
    } else {
        let tp = func.type_params.iter()
            .map(|t| format!("{} extends Comparable<{}>", t, t))
            .collect::<Vec<_>>()
            .join(", ");
        format!("<{}> ", tp)
    };

    out.push_str(&format!(
        "    public static {}{} {}({}) {{\n",
        type_params, ret, func.name, params
    ));
    emit_body_java(&mut out, &func.body, &func.params, &func.output);
    out.push_str("    }\n");
    out
}

fn emit_main_function_java(func: &Bullet) -> String {
    let mut out = String::new();
    out.push_str("    public static void main(String[] args) {\n");
    emit_body_java(&mut out, &func.body, &func.params, &func.output);
    out.push_str("    }\n");
    out
}

fn emit_body_java(
    out: &mut String,
    body: &BulletBody,
    params: &[Param],
    output: &Option<OutputDecl>,
) {
    let fn_outputs = std::collections::HashMap::new();
    match body {
        BulletBody::Pipes(pipes) => {
            if pipes.is_empty() { return; }
            let env = crate::codegen::typeinfer::TypeEnv::seed(params, &fn_outputs);
            let last = pipes.len().saturating_sub(1);
            // Unlike C's `__arg_N` (scoped inside that pipe's own GNU
            // statement-expression), Java temps below land as plain
            // statements in the shared method body — so the counter has to
            // be unique across the whole method, not reset per pipe.
            let mut tmp_counter: usize = 0;
            for (i, pipe) in pipes.iter().enumerate() {
                // Handle builtin::name with implicit pipe inputs
                let expr_str = if let Expr::Atom(Atom::BuiltinNoArgs(name)) = &pipe.expr {
                    let synthetic_params: Vec<bullang::ast::Param> = pipe.inputs
                        .iter()
                        .map(|input| {
                            let param_name = match input {
                                Expr::Atom(Atom::Ident(s)) => s.clone(),
                                // Atomic literals are safe to splice
                                // directly with no temp needed — and doing
                                // so keeps their literal-ness visible to
                                // callers like close/fd_in/fd_out's Java
                                // arms, which special-case a literal fd of
                                // 0/1/2 to skip referencing BuNative
                                // entirely. Hoisting these into a temp (as
                                // the fallback below does) would still be
                                // correct but would hide that literal-ness
                                // behind a variable name, silently forcing
                                // every project that ever passes a bare
                                // `1`/`2` fd into requiring the native lib.
                                Expr::Atom(Atom::Integer(_))
                                | Expr::Atom(Atom::Float(_))
                                | Expr::Atom(Atom::StringLit(_)) => emit_expr_java(input),
                                // Anything else (a field access, a nested
                                // call, ...) — declare a real temporary
                                // above the call and reference that instead.
                                // Previously this fell back to a made-up
                                // `__pipe_arg_N` name with no matching
                                // declaration anywhere, so any multi-arg
                                // implicit call with a non-ident input (e.g.
                                // `(path, "w"): builtin::open`) produced
                                // Java that referenced an undefined
                                // variable — see codegen_c.rs's equivalent
                                // block, which already does this correctly.
                                _ => {
                                    let tmp = format!("__arg_{}", tmp_counter);
                                    tmp_counter += 1;
                                    out.push_str(&format!(
                                        "        var {} = {};\n",
                                        tmp, emit_expr_java(input)
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
                    match crate::stdlib::emit_builtin(name, &synthetic_params, &Backend::Java) {
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
                            name,
                            args.iter().map(emit_expr_java).collect::<Vec<_>>().join(", ")
                        ),
                        crate::pipe::PipeRhs::Expr(expr) => emit_expr_java(expr),
                        // Handled above, before this branch is reached.
                        crate::pipe::PipeRhs::Builtin { .. } => unreachable!(),
                    }
                };
                let binding = pipe.binding.as_deref().unwrap_or("__v");
                if i == last {
                    let ret = output.as_ref()
                        .map(|o| bu_type_to_java(&o.ty))
                        .unwrap_or_else(|| "void".to_string());
                    if ret == "void" {
                        out.push_str(&format!("        {};\n", expr_str));
                    } else {
                        out.push_str(&format!("        var {} = {};\n", binding, expr_str));
                        out.push_str(&format!("        return {};\n", binding));
                    }
                } else {
                    out.push_str(&format!("        var {} = {};\n", binding, expr_str));
                }
            }
        }
        BulletBody::Natives(blocks) => {
            let block = blocks.iter().find(|b| b.backend == Backend::Java);
            match block {
                Some(b) => {
                    let base = b.code.lines()
                        .filter(|l| !l.trim().is_empty())
                        .map(|l| l.len() - l.trim_start().len())
                        .min()
                        .unwrap_or(0);
                    for line in b.code.lines() {
                        if line.trim().is_empty() {
                            out.push('\n');
                        } else {
                            let stripped = if line.len() >= base {
                                &line[base..]
                            } else {
                                line.trim_start()
                            };
                            out.push_str(&format!("        {}\n", stripped));
                        }
                    }
                }
                None => {
                    if let Some(b) = blocks.first() {
                        out.push_str(&format!(
                            "        // ERROR: '@{}' cannot be used in a Java build — use '@java'\n",
                            b.backend.escape_keyword()
                        ));
                    }
                }
            }
        }
        BulletBody::Builtin(name) => {
            use crate::stdlib;
            match stdlib::emit_builtin(name, params, &Backend::Java) {
                Ok(code) => {
                    let ret = output.as_ref()
                        .map(|o| bu_type_to_java(&o.ty))
                        .unwrap_or_else(|| "void".to_string());
                    if ret == "void" {
                        out.push_str(&format!("        {};\n", code));
                    } else {
                        out.push_str(&format!("        return {};\n", code));
                    }
                }
                Err(e) => out.push_str(&format!("        // ERROR: {}\n", e)),
            }
        }
    }
}

// ── Expression emitters ───────────────────────────────────────────────────────

fn emit_expr_java(expr: &Expr) -> String {
    match expr {
        Expr::Atom(a) => emit_atom_java(a),
        Expr::BinOp(b) => format!(
            "{} {} {}",
            emit_atom_java(&b.lhs),
            b.op,
            emit_atom_java(&b.rhs)
        ),
        Expr::Tuple(exprs) => {
            // Emit as Object array — simplest portable tuple representation.
            let elems = exprs.iter().map(emit_expr_java).collect::<Vec<_>>().join(", ");
            format!("new Object[]{{{}}}", elems)
        }
    }
}

fn emit_atom_java(atom: &Atom) -> String {
    match atom {
        Atom::Ident(s) => s.clone(),
        Atom::Integer(n) => n.to_string(),
        Atom::Float(n) => {
            // Java requires explicit double literals to end with `d` or have a decimal point.
            let s = n.to_string();
            if s.contains('.') { s } else { format!("{}d", s) }
        }
        Atom::StringLit(s) => format!("\"{}\"", s),
        Atom::Interp(template) => {
            // Java uses String.format with %s for each interpolated variable.
            let (fmt_str, vars) = interp_to_format(template);
            if vars.is_empty() {
                format!("\"{}\"", fmt_str)
            } else {
                format!("String.format(\"{}\", {})", fmt_str, vars.join(", "))
            }
        }
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
            match crate::pipe::inline_builtin(name, args, &Backend::Java, &emit_expr_java) {
                Ok(code) => code,
                Err(e)   => format!("/* ERROR: {e} */"),
            },
        Atom::Unary { op, rhs } => format!("({}{})", op, emit_atom_java(rhs)),
        Atom::FieldAccess { base, fields } => format!("{}.{}", base, fields.join(".")),
        Atom::Index { base, idx } =>
            // Java String.charAt returns char; cast to String via Character.toString.
            format!("Character.toString({}.charAt((int)({})))", base, emit_expr_java(idx)),
        Atom::Slice { base, from, to } =>
            format!("{}.substring((int)({}), (int)({}))", base, emit_expr_java(from), emit_expr_java(to)),
        Atom::EnumVariant { ty, variant } => format!("{}.{}", ty, variant),
    }
}


/// `"Hello {name}!"` → `("Hello %s!", ["name"])`
fn interp_to_format(template: &str) -> (String, Vec<&str>) {
    let mut fmt_str = String::new();
    let mut vars    = Vec::new();
    let mut rest    = template;
    while !rest.is_empty() {
        if let Some(open) = rest.find('{') {
            fmt_str.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            if let Some(close) = after.find('}') {
                let name = &after[..close];
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    fmt_str.push_str("%s");
                    vars.push(name);
                    rest = &after[close + 1..];
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

pub fn bu_type_to_java(ty: &BuType) -> String {
    match ty {
        BuType::Named(s)     => rust_type_to_java(s),
        BuType::Tuple(inner) => {
            // Represent as Object[] — simplest portable approach without codegen of named records.
            let _ = inner;
            "Object[]".to_string()
        }
        BuType::Unknown       => "Object".to_string(),
    }
}

fn rust_type_to_java(s: &str) -> String {
    let s: String = s.split_whitespace().collect();
    match s.as_str() {
        "i8"    => "byte".to_string(),
        "i16"   => "short".to_string(),
        "i32"   => "int".to_string(),
        "i64"   => "long".to_string(),
        "i128"  => "long".to_string(),
        "isize" => "long".to_string(),
        // Java has no unsigned — widen to next signed type.
        "u8"    => "int".to_string(),
        "u16"   => "int".to_string(),
        "u32"   => "long".to_string(),
        "u64"   => "long".to_string(),
        "u128"  => "long".to_string(),
        "usize" => "long".to_string(),
        "f32"   => "float".to_string(),
        "f64"   => "double".to_string(),
        "bool"  => "boolean".to_string(),
        "char"  => "char".to_string(),
        "String" | "&str" => "String".to_string(),
        "()"    => "void".to_string(),
        other   => translate_java_generic(other),
    }
}

fn translate_java_generic(s: &str) -> String {
    if s.starts_with("Vec[") && s.ends_with(']') {
        let inner = box_java_primitive(rust_type_to_java(&s[4..s.len() - 1]));
        return format!("java.util.ArrayList<{}>", inner);
    }
    if s.starts_with("HashMap[") && s.ends_with(']') {
        let inner = &s[8..s.len() - 1];
        let parts: Vec<&str> = inner.splitn(2, ',').collect();
        if parts.len() == 2 {
            let k = box_java_primitive(rust_type_to_java(parts[0].trim()));
            let v = box_java_primitive(rust_type_to_java(parts[1].trim()));
            return format!("java.util.HashMap<{}, {}>", k, v);
        }
    }
    if s.starts_with("Option[") && s.ends_with(']') {
        // Nullable — no wrapper; just the boxed type.
        return box_java_primitive(rust_type_to_java(&s[7..s.len() - 1]));
    }
    if s.starts_with("Fn[") && s.ends_with(']') {
        return translate_fn_type_java(s);
    }
    if s.starts_with('&') {
        return rust_type_to_java(s[1..].trim());
    }
    // Unknown — pass through.
    s.to_string()
}

/// Java generics require boxed (reference) types — convert primitives.
fn box_java_primitive(s: String) -> String {
    match s.as_str() {
        "byte"    => "Byte".to_string(),
        "short"   => "Short".to_string(),
        "int"     => "Integer".to_string(),
        "long"    => "Long".to_string(),
        "float"   => "Float".to_string(),
        "double"  => "Double".to_string(),
        "boolean" => "Boolean".to_string(),
        "char"    => "Character".to_string(),
        other     => other.to_string(),
    }
}

fn translate_fn_type_java(s: &str) -> String {
    // Fn[T -> U]  →  java.util.function.Function<T,U>
    // Fn[-> U]    →  java.util.function.Supplier<U>
    // Fn[T ->]    →  java.util.function.Consumer<T>
    // Fn[]        →  java.lang.Runnable
    let inner = s.trim_start_matches("Fn[").trim_end_matches(']');
    if inner.trim().is_empty() {
        return "java.lang.Runnable".to_string();
    }
    if let Some(arrow) = inner.find("->") {
        let params_str = inner[..arrow].trim();
        let ret_str    = inner[arrow + 2..].trim();
        let ret_empty  = ret_str.is_empty() || ret_str == "()";
        let params: Vec<String> = if params_str.is_empty() { vec![] }
            else {
                params_str.split(',')
                    .map(|p| box_java_primitive(rust_type_to_java(p.trim())))
                    .collect()
            };
        match (params.len(), ret_empty) {
            (0, true)  => "java.lang.Runnable".to_string(),
            (0, false) => format!("java.util.function.Supplier<{}>",
                box_java_primitive(rust_type_to_java(ret_str))),
            (1, true)  => format!("java.util.function.Consumer<{}>", params[0]),
            (1, false) => format!("java.util.function.Function<{},{}>",
                params[0],
                box_java_primitive(rust_type_to_java(ret_str))),
            (2, false) => format!("java.util.function.BiFunction<{},{},{}>",
                params[0], params[1],
                box_java_primitive(rust_type_to_java(ret_str))),
            _ => "java.util.function.Function<Object,Object> /* multi-arg */".to_string(),
        }
    } else {
        // No arrow — treat as Supplier
        format!("java.util.function.Supplier<{}>",
            box_java_primitive(rust_type_to_java(inner.trim())))
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────
// to_pascal_case is re-used from codegen_go — no duplicate defined here.
