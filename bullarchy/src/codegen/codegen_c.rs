//! C code generation backend.
//!
//! Produces a self-contained C source file per Bullang source file,
//! a shared header (<crate>.h) that exposes all public functions,
//! and a Makefile to compile the project.

use bullang::ast::*;
use crate::codegen::typeinfer::{TypeEnv, collect_fn_output_types};
use std::collections::{BTreeSet, HashMap};

/// The Vec[T]/HashMap[K,V] runtime (vec_t/map_t) — normally written as a
/// companion foreign_types.h in multi-file project builds. Bare/single-file
/// mode has no companion file to include, so when it's needed it gets
/// inlined directly into the .c file instead, right after the #includes —
/// same placement as a hoisted builtin helper.
const FOREIGN_TYPES_C: &str = include_str!("../foreign_types.h");

// ── Hoisted includes ─────────────────────────────────────────────────────────

fn emit_c_includes(out: &mut String, file: &SourceFile) {
    for include in crate::codegen::hoist::requirements(file, &Backend::C).imports {
        out.push_str(include);
        out.push('\n');
    }
}

// ── Hoisted helper functions ────────────────────────────────────────────────

/// Emitted once, above every user function — including `main` — because C
/// requires definition before use.
fn emit_c_helpers(out: &mut String, file: &SourceFile) {
    for helper in crate::codegen::hoist::requirements(file, &Backend::C).helpers {
        out.push_str(helper);
        out.push('\n');
    }
}

// ── Void-returning callee detection ─────────────────────────────────────────

/// True if `ty` is Bullang's unit type `()`.
///
/// pub(crate): also used by codegen.rs (Rust) — same check, needed there to
/// fix the analogous BulletBody::Builtin(name) shorthand bug (a unit-typed
/// function whose sole body is a builtin call must not emit that call as a
/// tail expression when the builtin's Rust code evaluates to a non-unit
/// value, e.g. `close`'s `{ ...; 0i32 }`).
pub(crate) fn type_is_unit(ty: &BuType) -> bool {
    matches!(ty, BuType::Named(s) if s == "()")
}

/// True if `func`'s declared Bullang return type is unit (including no
/// declared output at all, which defaults to unit).
pub(crate) fn output_is_unit(func: &Bullet) -> bool {
    match &func.output {
        None    => true,
        Some(o) => type_is_unit(&o.ty),
    }
}

/// Names of every unit-returning function declared in `file`. A pipe that
/// calls one of these (e.g. `shout(...)` where `shout` itself ends in a
/// `builtin::out` call) compiles to a `void` C function — trying to bind
/// its result with `__auto_type x = shout(...);` doesn't compile, so the
/// call must be emitted as a bare statement instead.
///
/// Scoped to functions declared in the SAME file: in the multi-file project
/// build, a caller in main.bu invoking a unit-returning function defined in
/// a different .bu module won't be caught by this — that cross-file case
/// isn't covered yet.
pub(crate) fn collect_unit_functions(file: &SourceFile) -> BTreeSet<&str> {
    file.bullets.iter()
        .filter(|f| output_is_unit(f))
        .map(|f| f.name.as_str())
        .collect()
}

// ── Source file → C ───────────────────────────────────────────────────────────

pub fn emit_source_c(file: &SourceFile, header_name: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("#include \"{}\"\n", header_name));
    if needs_stdlib(file) {
        out.push_str("#include <stdlib.h>\n");
    }
    if needs_string_h(file) {
        out.push_str("#include <string.h>\n");
    }
    emit_c_includes(&mut out, file);
    if out.ends_with('\n') && !out.ends_with("\n\n") {
        out.push('\n');
    }
    emit_c_helpers(&mut out, file);

    let unit_fns = collect_unit_functions(file);
    let fn_outputs = collect_fn_output_types(file);
    for func in &file.bullets {
        out.push_str(&emit_function_c(func, &unit_fns, &fn_outputs));
        out.push('\n');
    }
    out
}

/// Single-file mode: emit a self-contained `.c` with no companion `.h`.
/// Includes and forward declarations are inlined at the top.
/// Bare single-file mode: only the function bodies plus any hoisted builtin
/// includes/helpers — no forward declarations, no other preamble.
pub fn emit_bare_c(file: &SourceFile) -> String {
    let mut out = String::new();
    emit_c_includes(&mut out, file);
    out.push('\n');
    out.push_str(BU_STR_MAX_DEF);
    out.push('\n');
    if needs_foreign_types(file) {
        out.push_str(FOREIGN_TYPES_C);
        out.push('\n');
    }
    emit_c_helpers(&mut out, file);
    let unit_fns = collect_unit_functions(file);
    let fn_outputs = collect_fn_output_types(file);
    for func in &file.bullets {
        if func.name == "main" {
            out.push_str(&emit_main_function_c(func, &unit_fns, &fn_outputs));
        } else {
            out.push_str(&emit_function_c(func, &unit_fns, &fn_outputs));
        }
        out.push('\n');
    }
    out
}

// ── Struct emitter ────────────────────────────────────────────────────────────

pub fn emit_struct_c(s: &bullang::ast::StructDef) -> String {
    let mut out = String::new();
    out.push_str(&format!("typedef struct {{\n"));
    for field in &s.fields {
        out.push_str(&format!("    {} {};\n", bu_type_to_c(&field.ty), field.name));
    }
    out.push_str(&format!("}} {};\n", s.name));
    out
}

pub fn emit_enum_c(e: &bullang::ast::EnumDef) -> String {
    let mut out = String::new();
    out.push_str("typedef enum {\n");
    for v in &e.variants {
        out.push_str(&format!("    {},\n", v.name));
    }
    out.push_str(&format!("}} {};\n", e.name));
    out
}

// ── foreign_types.h detection ─────────────────────────────────────────────────

/// Returns true if the source file uses any type that requires foreign_types.h,
/// OR calls a builtin (max, min, args) whose C implementation uses vec_t
/// internally even when Vec[T] never appears in the calling function's own
/// signature (e.g. an intermediate binding that's never returned or taken
/// as a param).
pub fn needs_foreign_types(file: &SourceFile) -> bool {
    // The builtin check that used to live here listed `max`, `min` and `args`
    // as needing the vec_t runtime. None of them do any more: max and min take
    // two integers, and `args` is indexed rather than returning a list.
    file.bullets.iter().any(|b| {
        b.params.iter().any(|p| type_needs_foreign(&p.ty))
            || type_needs_foreign(
                &b.output.as_ref().map(|o| &o.ty)
                    .unwrap_or(&bullang::ast::BuType::unit())
            )
    })
}

pub fn needs_generic_types(file: &SourceFile) -> bool {
    file.bullets.iter().any(|b| !b.type_params.is_empty())
}

/// `<stdbool.h>` — needed when `bool` appears anywhere in the public API.
pub fn needs_stdbool(file: &SourceFile) -> bool {
    file.bullets.iter().any(|b| {
        type_is_bool(&b.output.as_ref().map(|o| &o.ty).unwrap_or(&bullang::ast::BuType::unit())) || b.params.iter().any(|p| type_is_bool(&p.ty))
    })
}

fn type_is_bool(ty: &BuType) -> bool {
    match ty {
        BuType::Named(s)    => s == "bool",
        BuType::Tuple(ts)   => ts.iter().any(type_is_bool),
        BuType::Unknown     => false,
    }
}

/// `<stdlib.h>` — needed when native blocks reference `malloc`, `free`, `calloc`,
/// `realloc`, or `exit`, or when `Option[T]` types appear (nullable pointer idiom).
pub fn needs_stdlib(file: &SourceFile) -> bool {
    const MARKERS: &[&str] = &["malloc", "calloc", "realloc", "free", "exit", "abort", "NULL"];
    file.bullets.iter().any(|b| {
        any_type_needs_stdlib(&b.output.as_ref().map(|o| &o.ty).unwrap_or(&bullang::ast::BuType::unit()))
            || b.params.iter().any(|p| any_type_needs_stdlib(&p.ty))
            || native_blocks_contain(b, MARKERS)
    })
}

fn any_type_needs_stdlib(ty: &BuType) -> bool {
    match ty {
        BuType::Named(s) => s.starts_with("Option["),
        BuType::Tuple(ts)   => ts.iter().any(any_type_needs_stdlib),
        BuType::Unknown     => false,
    }
}

/// `<string.h>` — needed when any slice expression (`strndup`) or native block
/// references string functions.
pub fn needs_string_h(file: &SourceFile) -> bool {
    const MARKERS: &[&str] = &["strndup", "strlen", "strcpy", "strcat", "strcmp",
                                "strncpy", "memcpy", "memmove", "memset", "memcmp"];
    file.bullets.iter().any(|b| {
        body_has_slice(&b.body) || native_blocks_contain(b, MARKERS)
    })
}

fn body_has_slice(body: &BulletBody) -> bool {
    match body {
        BulletBody::Pipes(pipes) => pipes.iter().any(|p| expr_has_slice(&p.expr)),
        _ => false,
    }
}

fn expr_has_slice(expr: &Expr) -> bool {
    match expr {
        Expr::Atom(a)      => atom_has_slice(a),
        Expr::BinOp(b)     => atom_has_slice(&b.lhs) || atom_has_slice(&b.rhs),
        Expr::Tuple(exprs) => exprs.iter().any(expr_has_slice),
    }
}

fn atom_has_slice(atom: &Atom) -> bool {
    matches!(atom, Atom::Slice { .. })
}

/// Returns true if any native block in `bullet` contains any of the given substrings.
fn native_blocks_contain(bullet: &Bullet, markers: &[&str]) -> bool {
    match &bullet.body {
        BulletBody::Natives(blocks) => blocks.iter().any(|b| {
            markers.iter().any(|m| b.code.contains(m))
        }),
        _ => false,
    }
}

fn type_needs_foreign(ty: &BuType) -> bool {
    match ty {
        BuType::Named(s) => s.starts_with("Vec[") || s.starts_with("HashMap["),
        BuType::Tuple(ts)   => ts.iter().any(type_needs_foreign),
        BuType::Unknown     => false,
    }
}

pub fn emit_header_c(
    module_name:  &str,
    source_files: &[(String, &SourceFile)],
    includes:     &[String],
    structs:      &[bullang::ast::StructDef],
    enums:        &[bullang::ast::EnumDef],
) -> String {
    let guard    = format!("{}_H", module_name.to_uppercase().replace('-', "_"));
    let needs_ft   = source_files.iter().any(|(_, sf)| needs_foreign_types(sf));
    let needs_gen  = source_files.iter().any(|(_, sf)| needs_generic_types(sf));
    let needs_bool = source_files.iter().any(|(_, sf)| needs_stdbool(sf));
    let mut out    = String::new();

    out.push_str(&format!("#ifndef {}\n#define {}\n\n", guard, guard));
    out.push_str("#include <stdint.h>\n");
    out.push('\n');
    out.push_str(BU_STR_MAX_DEF);
    if needs_bool {
        out.push_str("#include <stdbool.h>\n");
    }
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

    // Tuple typedefs — one per unique combination used anywhere in this module
    let tuple_types = collect_tuple_types_c(source_files);
    for inner in &tuple_types {
        out.push_str(&emit_tuple_struct_c(inner));
        out.push('\n');
    }

    for (filename, sf) in source_files {
        out.push_str(&format!("/* {} */\n", filename));
        for func in &sf.bullets {
            let (ret, params) = c_signature(func);
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
    // <stdio.h> is always included in main — assert expressions emit fprintf(stderr,...)
    // and virtually every entry point does some I/O.
    out.push_str("#include <stdio.h>\n");
    if needs_stdlib(file) {
        out.push_str("#include <stdlib.h>\n");
    }
    emit_c_includes(&mut out, file);
    out.push_str(&format!("#include \"{}\"\n\n", header_name));
    emit_c_helpers(&mut out, file);

    let unit_fns = collect_unit_functions(file);
    let fn_outputs = collect_fn_output_types(file);
    for func in &file.bullets {
        if func.name == "main" {
            out.push_str(&emit_main_function_c(func, &unit_fns, &fn_outputs));
        } else {
            out.push_str(&emit_function_c(func, &unit_fns, &fn_outputs));
        }
        out.push('\n');
    }
    out
}
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

fn emit_function_c(func: &Bullet, unit_fns: &BTreeSet<&str>, fn_outputs: &HashMap<String, BuType>) -> String {
    let mut out = String::new();

    if func.type_params.is_empty() {
        let (ret, params) = c_signature(func);
        out.push_str(&format!("{} {}({}) {{\n", ret, func.name, params));
        // A String-returning function writes into its destination and returns
        // nothing, so its body is emitted as if it returned unit.
        let string_output = if returns_string(func) {
            func.output.as_ref().map(|o| o.name.as_str())
        } else {
            None
        };
        emit_body_c_out(
            &mut out, &func.body, &func.params, &Backend::C,
            ret == "void", unit_fns, fn_outputs, string_output,
        );
    } else {
        // Generic function — type params become BuVal.
        out.push_str("#include \"bu_generic.h\"\n");
        let params = c_generic_param_list(&func.params, &func.type_params);
        let ret    = c_generic_type(&func.output.as_ref().map(|o| &o.ty).unwrap_or(&bullang::ast::BuType::unit()), &func.type_params);
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
                } else if let Some(binding) = pipe.binding.as_deref() {
                    out.push_str(&format!("    BuVal {} = {};\n", binding, expr_str));
                } else {
                    // `-> {}` — explicit discard, no declaration at all
                    // (see emit_body_c for why fabricating a fallback name
                    // like `_` is wrong here).
                    out.push_str(&format!("    {};\n", expr_str));
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
            let fields: Vec<String> = exprs.iter().enumerate()
                .map(|(i, e)| format!(".v{} = {}", i, emit_expr_c_generic(e, tp)))
                .collect();
            format!("({{{}}})", fields.join(", "))
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

fn emit_main_function_c(func: &Bullet, unit_fns: &BTreeSet<&str>, fn_outputs: &HashMap<String, BuType>) -> String {
    let mut out = String::new();
    out.push_str("int main(void) {\n");
    emit_body_c(&mut out, &func.body, &func.params, &Backend::C, true, unit_fns, fn_outputs);
    // If body doesn't have a return, add one
    out.push_str("    return 0;\n");
    out.push_str("}\n");
    out
}

pub fn emit_body_c(out: &mut String, body: &BulletBody, params: &[Param], backend: &Backend, returns_unit: bool, unit_fns: &BTreeSet<&str>, fn_outputs: &HashMap<String, BuType>) {
    emit_body_c_out(out, body, params, backend, returns_unit, unit_fns, fn_outputs, None)
}

/// `string_output` is the name of the caller-supplied destination, for a
/// function that returns a String. See `c_signature`.
pub fn emit_body_c_out(out: &mut String, body: &BulletBody, params: &[Param], backend: &Backend, returns_unit: bool, unit_fns: &BTreeSet<&str>, fn_outputs: &HashMap<String, BuType>, string_output: Option<&str>) {
    match body {
        BulletBody::Pipes(pipes) => {
            if pipes.is_empty() { return; }
            let last = pipes.len().saturating_sub(1);
            let mut env = TypeEnv::seed(params, fn_outputs);
            for (i, pipe) in pipes.iter().enumerate() {
                let mut callee_is_unit = false;

                // A builtin's arguments, as text. Anything that is not a
                // plain name is hoisted into a temporary first, because the
                // builtin splices the text it is given straight into its
                // output and a made-up name would not be declared anywhere.
                let builtin_params = |out: &mut String, env: &TypeEnv, args: &[Expr]| {
                    let mut ps: Vec<bullang::ast::Param> = Vec::new();
                    for (idx, input) in args.iter().enumerate() {
                        let inferred_ty = env.infer(input);
                        let name = match input {
                            Expr::Atom(Atom::Ident(s)) => s.clone(),
                            _ => {
                                let tmp = format!("__arg_{}", idx);
                                out.push_str(&format!(
                                    "    __auto_type {} = {};\n",
                                    tmp, emit_call_arg_c(input)
                                ));
                                tmp
                            }
                        };
                        ps.push(bullang::ast::Param { name, ty: inferred_ty });
                    }
                    ps
                };

                // ── Decision 19: string-returning builtins in C ──────────
                //
                // C has no owning string type, so these do not produce an
                // expression at all: the caller declares the destination and
                // the builtin fills it. That makes the bullet a declaration
                // followed by a statement, not `binding = <rhs>;`, so it has
                // to be decided before an expression is built.
                //
                //     char t[ft_strlen(s) + 1];
                //     ft_trim(t, s);
                //
                // No malloc, and no writing through a pointer that might be a
                // string literal — which is what the old code did, and is
                // undefined behaviour.
                if let crate::pipe::PipeRhs::Builtin { name, args } = crate::pipe::classify(pipe) {
                    if crate::stdlib::returns_string_in_c(name) {
                        let binding = pipe.binding.as_deref().unwrap_or("__discard");
                        let ps = builtin_params(out, &env, args);
                        match crate::stdlib::emit_c_dest(name, binding, &ps) {
                            Ok(Some(dest)) => {
                                // On the last bullet of a String-returning
                                // function the destination is the caller's,
                                // passed in as the first parameter — so the
                                // builtin writes straight into it and there is
                                // no local buffer to declare, and nothing that
                                // could outlive the frame.
                                if i == last && string_output.is_some() {
                                    let out_name = string_output.unwrap();
                                    let redirected = crate::stdlib::emit_c_dest(name, out_name, &ps)
                                        .ok().flatten();
                                    if let Some(d) = redirected {
                                        out.push_str(&format!("    {};\n", d.call));
                                        continue;
                                    }
                                }
                                out.push_str(&format!(
                                    "    char {}[{}];\n", binding, dest.size
                                ));
                                out.push_str(&format!("    {};\n", dest.call));
                                env.bind(binding, BuType::Named("String".to_string()));
                                continue;
                            }
                            Ok(None) => {}
                            Err(e) => {
                                out.push_str(&format!("    /* ERROR: {e} */\n"));
                                continue;
                            }
                        }
                    }
                }

                // A call to a String-returning function follows the same
                // convention as a string builtin: the caller supplies the
                // destination. See `c_signature`.
                if let crate::pipe::PipeRhs::Call { name, args } = crate::pipe::classify(pipe) {
                    if fn_outputs.get(name).is_some_and(type_is_string) {
                        let arg_list = args.iter()
                            .map(emit_call_arg_c)
                            .collect::<Vec<_>>()
                            .join(", ");
                        let sep = if arg_list.is_empty() { "" } else { ", " };
                        // On the last bullet the destination is the caller's
                        // own, already a parameter — no local buffer at all.
                        if i == last && string_output.is_some() {
                            out.push_str(&format!(
                                "    {}({}{}{});\n",
                                name, string_output.unwrap(), sep, arg_list
                            ));
                            continue;
                        }
                        let binding = pipe.binding.as_deref().unwrap_or("__discard");
                        out.push_str(&format!("    char {}[{}];\n", binding, BU_STR_MAX));
                        out.push_str(&format!(
                            "    {}({}{}{});\n", name, binding, sep, arg_list
                        ));
                        env.bind(binding, BuType::Named("String".to_string()));
                        continue;
                    }
                }

                let expr_str = match crate::pipe::classify(pipe) {
                    crate::pipe::PipeRhs::Builtin { name, args } => {
                        let ps = builtin_params(out, &env, args);
                        match crate::stdlib::emit_builtin(name, &ps, backend) {
                            Ok(code) => code,
                            Err(e)   => format!("/* ERROR: {e} */"),
                        }
                    }
                    // See pipe.rs: the pipe's inputs are arguments only when
                    // the expression is a bare callee.
                    crate::pipe::PipeRhs::Call { name, args } => {
                        callee_is_unit = unit_fns.contains(name);
                        format!(
                            "{}({})",
                            name,
                            args.iter().map(emit_call_arg_c).collect::<Vec<_>>().join(", ")
                        )
                    }
                    crate::pipe::PipeRhs::Expr(expr) => {
                        if let Expr::Atom(Atom::Call { name, .. }) = expr {
                            callee_is_unit = unit_fns.contains(name.as_str());
                        }
                        emit_expr_c(expr)
                    }
                };
                // A unit-returning function (Bullang `-> ()`, which includes
                // `main` — its C signature is forced to `int` for a valid
                // entry point, but its Bullang-level return type is still
                // unit) never turns its last pipe into `return expr;`: the
                // pipe's value isn't meaningful to the caller, and for
                // `main` specifically the expression's C type has no
                // relationship to the required `int` return type at all.
                if i == last && !returns_unit {
                    out.push_str(&format!("    return {};\n", expr_str));
                } else if callee_is_unit || pipe.binding.is_none() {
                    // Two distinct reasons to skip binding entirely:
                    // - callee_is_unit: the callee compiles to a `void` C
                    //   function, nothing to bind.
                    // - pipe.binding.is_none(): the Bullang source wrote
                    //   `-> {}` — an explicit discard. A bare statement
                    //   here relies on the builtin/function not wrapping
                    //   its result in an explicit cast — a cast used as a
                    //   discarded statement trips -Wunused-value even
                    //   though a plain call never does. See fd_out.rs,
                    //   open.rs, time.rs for builtins where the cast used
                    //   to be there and was removed for exactly this reason.
                    out.push_str(&format!("    {};\n", expr_str));
                } else {
                    let binding = pipe.binding.as_deref().unwrap();
                    env.bind(binding, env.infer(&pipe.expr));
                    out.push_str(&format!("    __auto_type {} = {};\n", binding, expr_str));
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
            match crate::stdlib::emit_builtin(name, params, backend) {
                // A unit-returning function (`-> ()`, including `main`)
                // never turns its sole builtin call into `return expr;` —
                // same reasoning as the `i == last && !returns_unit` branch
                // in the Pipes path above. A bare statement here is safe
                // from -Wunused-value as long as the builtin itself has no
                // needless cast wrapping its result (see fd_out.rs, open.rs,
                // time.rs).
                Ok(code) if returns_unit => out.push_str(&format!("    {};\n", code)),
                Ok(code)                 => out.push_str(&format!("    return {};\n", code)),
                Err(e)                   => out.push_str(&format!("    /* ERROR: {} */\n", e)),
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
            // Emit as a compound literal: (Tuple_T_U){ .v0 = x, .v1 = y }
            let fields: Vec<String> = exprs.iter().enumerate()
                .map(|(i, e)| format!(".v{} = {}", i, emit_expr_c(e)))
                .collect();
            format!("({{{}}})", fields.join(", "))
        }
    }
}

/// Emit a function-call argument. String literals are wrapped as a
/// compound literal `(char[]){"..."}` instead of a bare `"..."`.
///
/// Bullang strings are plain `char*` throughout the C backend, and several
/// builtins (e.g. `to_upper`) mutate their argument in place rather than
/// allocating a copy — cheap, but it means a bare string literal, which
/// points into read-only `.rodata`, crashes the moment it's written to.
/// `(char[]){"..."}` is a stack-allocated, mutable array with automatic
/// storage duration scoped to the enclosing block — no heap allocation,
/// and safe as long as the result doesn't outlive that block (the same
/// rule that already applies to any pointer into a C stack frame).
fn emit_call_arg_c(expr: &Expr) -> String {
    if let Expr::Atom(Atom::StringLit(s)) = expr {
        format!("(char[]){{\"{}\"}}", s)
    } else {
        emit_expr_c(expr)
    }
}

pub fn emit_atom_c(atom: &Atom) -> String {
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
            match crate::pipe::inline_builtin(name, args, &Backend::C, &emit_expr_c) {
                Ok(code) => code,
                Err(e)   => format!("/* ERROR: {e} */"),
            },
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
                CallArg::Value(s) => s.clone(),
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
        // C closures via GCC compound statement with nested function.
        // The nested function is named __bu_closure_N (unique per call site).
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

// ── Tuple struct support ──────────────────────────────────────────────────────

/// `Tuple[i32, f64]` → `Tuple_i32_f64`
pub fn tuple_c_name(inner: &[BuType]) -> String {
    let parts: Vec<String> = inner.iter()
        .map(|t| bu_type_to_c(t).replace(['*', ' ', '[', ']'], "_").trim_matches('_').to_string())
        .collect();
    format!("Tuple_{}", parts.join("_"))
}

/// Emit a `typedef struct` for a tuple combination, e.g.:
/// ```c
/// typedef struct { int32_t v0; double v1; } Tuple_i32_f64;
/// ```
pub fn emit_tuple_struct_c(inner: &[BuType]) -> String {
    let name   = tuple_c_name(inner);
    let fields: String = inner.iter().enumerate()
        .map(|(i, t)| format!("    {} v{};\n", bu_type_to_c(t), i))
        .collect();
    format!("typedef struct {{\n{}}} {};\n", fields, name)
}

/// Collect all unique Tuple type combinations used in a set of source files.
pub fn collect_tuple_types_c(source_files: &[(String, &SourceFile)]) -> Vec<Vec<BuType>> {
    let mut seen: Vec<Vec<BuType>> = Vec::new();

    fn scan(ty: &BuType, seen: &mut Vec<Vec<BuType>>) {
        if let BuType::Tuple(inner) = ty {
            if !seen.contains(inner) {
                seen.push(inner.clone());
            }
        }
    }

    for (_, sf) in source_files {
        for func in &sf.bullets {
            scan(&func.output.as_ref().map(|o| &o.ty).unwrap_or(&bullang::ast::BuType::unit()), &mut seen);
            for p in &func.params { scan(&p.ty, &mut seen); }
        }
    }
    seen
}

// ── Type mapping: Bullang → C ─────────────────────────────────────────────────

pub fn bu_type_to_c(ty: &BuType) -> String {
    match ty {
        BuType::Named(s)     => rust_type_to_c(s),
        BuType::Tuple(ts)    => tuple_c_name(ts),
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

// ── String returns in C ───────────────────────────────────────────────────
//
// C has no owning string type, so a function cannot return one: the buffer
// would have to live somewhere, and every option except the caller's frame is
// either a leak or a dangling pointer.
//
// So a Bullang function returning String compiles to a C function returning
// `void` that writes into a destination the caller supplies first — the same
// `ft_strcpy` convention the string builtins use (decision 19), applied to
// user functions for consistency:
//
//     let shout(s: String) -> loud: String        →   void shout(char *loud, const char *s)
//
// The caller declares the buffer immediately before the call. Unlike a
// builtin, a user function's output length cannot be computed from its inputs
// — the body could do anything — so the buffer is a fixed ceiling rather than
// an exact size. That ceiling is the one real cost of this convention.

/// The size of a destination buffer for a value returned by a user function.
pub const BU_STR_MAX: &str = "BU_STR_MAX";

pub const BU_STR_MAX_DEF: &str =
    "/* A user function's output length cannot be derived from its inputs, so a
        destination for one is a fixed ceiling rather than an exact size. */
     #define BU_STR_MAX 4096
";

/// True if `ty` is Bullang's `String`.
pub fn type_is_string(ty: &BuType) -> bool {
    matches!(ty, BuType::Named(n) if n == "String")
}

/// True if `func` returns a String, and so takes its destination first.
pub fn returns_string(func: &Bullet) -> bool {
    func.output.as_ref().is_some_and(|o| type_is_string(&o.ty))
}

/// The C signature of `func` — return type, name and parameter list.
///
/// One function so the header and the definition cannot disagree, which is
/// what went wrong when identifier escaping was applied to only one of them.
pub fn c_signature(func: &Bullet) -> (String, String) {
    let ret = bu_type_to_c(
        &func.output.as_ref().map(|o| &o.ty)
            .unwrap_or(&bullang::ast::BuType::unit())
    );
    if returns_string(func) {
        let dest = func.output.as_ref().map(|o| o.name.as_str()).unwrap_or("__dest");
        let rest = func.params.iter()
            .map(|p| format!("{} {}", bu_type_to_c(&p.ty), p.name))
            .collect::<Vec<_>>();
        let mut all = vec![format!("char *{dest}")];
        all.extend(rest);
        return ("void".to_string(), all.join(", "));
    }
    (ret, c_param_list(&func.params))
}
