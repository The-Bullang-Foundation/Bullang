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

use crate::ast::*;

// ── Source file → Go ──────────────────────────────────────────────────────────

pub fn emit_source_go(file: &SourceFile, package: &str) -> String {
    let pkg  = sanitize_go_pkg(package);
    let mut out = String::new();
    out.push_str(&format!("package {}\n\n", pkg));

    let imports = needed_imports(file);
    if !imports.is_empty() {
        out.push_str("import (\n");
        for imp in &imports { out.push_str(&format!("\t\"{}\"\n", imp)); }
        out.push_str(")\n\n");
    }

    for func in &file.bullets {
        out.push_str(&emit_function_go(func));
        out.push('\n');
    }
    out
}

/// Emit `types.go` — contains all inventory struct definitions and any
/// Tuple named structs needed as foreign type equivalents.
/// Called by build.rs whenever there are structs or Tuple types in the project.
pub fn emit_types_go(package: &str, structs: &[crate::ast::StructDef], enums: &[crate::ast::EnumDef], tuple_types: &[Vec<crate::ast::BuType>]) -> String {
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
pub fn tuple_go_name(inner: &[crate::ast::BuType]) -> String {
    let parts: Vec<String> = inner.iter().map(|t| {
        bu_type_to_go(t)
            .replace(['<', '>', '[', ']', ' ', ','], "_")
            .trim_matches('_')
            .to_string()
    }).collect();
    format!("Tuple_{}", parts.join("_"))
}

/// Collect all unique Tuple type combinations used across all source files.
pub fn collect_tuple_types(source_files: &[(String, &SourceFile)]) -> Vec<Vec<crate::ast::BuType>> {
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
            scan_type(&func.output.ty, &mut seen);
        }
    }
    seen
}

pub fn emit_struct_go(s: &crate::ast::StructDef) -> String {
    let mut out = String::new();
    out.push_str(&format!("type {} struct {{\n", to_pascal_case(&s.name)));
    for field in &s.fields {
        out.push_str(&format!("\t{} {}\n",
            to_pascal_case(&field.name), bu_type_to_go(&field.ty)));
    }
    out.push_str("}\n");
    out
}

pub fn emit_enum_go(e: &crate::ast::EnumDef) -> String {
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

pub fn emit_main_go(file: &SourceFile, _package: &str) -> String {
    let mut out = String::new();
    out.push_str("package main\n\n");

    // Imports needed by main file
    let imports = needed_imports(file);
    // Always include fmt if @go block is present (user likely uses fmt.Println etc.)
    let mut all_imports = imports;
    if !all_imports.contains(&"fmt".to_string()) {
        // Check if any native Go block is present
        let has_native = file.bullets.iter().any(|b| {
            matches!(&b.body, BulletBody::Natives(blocks) if blocks.iter().any(|nb| nb.backend == Backend::Go))
        });
        if has_native { all_imports.push("fmt".to_string()); }
    }

    if !all_imports.is_empty() {
        // Deduplicate while preserving order
        let mut seen = std::collections::HashSet::new();
        let unique: Vec<&str> = all_imports.iter()
            .filter(|s| seen.insert(s.as_str()))
            .map(|s| s.as_str()).collect();
        out.push_str("import (\n");
        for imp in unique { out.push_str(&format!("\t\"{}\"\n", imp)); }
        out.push_str(")\n\n");
    }

    for func in &file.bullets {
        if func.name == "main" {
            out.push_str(&emit_main_function_go(func));
        } else {
            out.push_str(&emit_function_go(func));
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

fn needed_imports(file: &SourceFile) -> Vec<String> {
    let mut imports = Vec::new();

    for func in &file.bullets {
        match &func.body {
            BulletBody::Builtin(name) => {
                match name.as_str() {
                    "sqrt" | "powf" | "pow" => { push_unique(&mut imports, "math"); }
                    "parse_i64" => { push_unique(&mut imports, "strconv"); push_unique(&mut imports, "strings"); }
                    "join" | "split_str" | "trim" | "to_upper" | "to_lower"
                    | "starts_with" | "ends_with" | "replace_str" => {
                        push_unique(&mut imports, "strings");
                    }
                    "to_string" => { push_unique(&mut imports, "fmt"); }
                    "sort" | "sort_by" => { push_unique(&mut imports, "sort"); }
                    _ => {}
                }
            }
            BulletBody::Natives(blocks) => {
                if let Some(b) = blocks.iter().find(|b| b.backend == Backend::Go) {
                    if b.code.contains("sort.")    { push_unique(&mut imports, "sort"); }
                    if b.code.contains("strings.") { push_unique(&mut imports, "strings"); }
                    if b.code.contains("math.")    { push_unique(&mut imports, "math"); }
                    if b.code.contains("fmt.")     { push_unique(&mut imports, "fmt"); }
                    if b.code.contains("strconv.") { push_unique(&mut imports, "strconv"); }
                    if b.code.contains("os.")      { push_unique(&mut imports, "os"); }
                    if b.code.contains("bufio.")   { push_unique(&mut imports, "bufio"); }
                }
            }
            BulletBody::Pipes(pipes) => {
                if pipes.iter().any(|p| pipe_has_interp(&p.expr)) {
                    push_unique(&mut imports, "fmt");
                }
            }
        }
        // Generic functions using constraints.Ordered need the constraints package
        if !func.type_params.is_empty() && go_needs_ordered(func) {
            push_unique(&mut imports, "golang.org/x/exp/constraints");
        }
    }
    imports.iter().map(|s| s.to_string()).collect()
}

fn pipe_has_interp(expr: &crate::ast::Expr) -> bool {
    match expr {
        Expr::Atom(Atom::Interp(_))     => true,
        Expr::Atom(_)                   => false,
        Expr::BinOp(b)                  => matches!(&b.lhs, Atom::Interp(_)) || matches!(&b.rhs, Atom::Interp(_)),
        Expr::Tuple(exprs)              => exprs.iter().any(pipe_has_interp),
    }
}

fn push_unique(v: &mut Vec<&'static str>, s: &'static str) {
    if !v.contains(&s) { v.push(s); }
}

// ── Function emitters ─────────────────────────────────────────────────────────

fn emit_function_go(func: &Bullet) -> String {
    let mut out   = String::new();
    let params    = go_param_list(&func.params);
    let ret       = bu_type_to_go(&func.output.ty);
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
    emit_body_go(&mut out, &func.body, &func.params, &func.output);
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

fn emit_main_function_go(func: &Bullet) -> String {
    let mut out = String::new();
    out.push_str("func main() {\n");
    emit_body_go(&mut out, &func.body, &func.params, &func.output);
    out.push_str("}\n");
    out
}

fn emit_body_go(out: &mut String, body: &BulletBody, params: &[Param], output: &OutputDecl) {
    match body {
        BulletBody::Pipes(pipes) => {
            if pipes.is_empty() { return; }
            let last = pipes.len().saturating_sub(1);
            for (i, pipe) in pipes.iter().enumerate() {
                let expr = emit_expr_go(&pipe.expr);
                out.push_str(&format!("\t{} := {}\n", pipe.binding, expr));
                if pipe.propagate {
                    // Go has no ? — emit an explicit nil/error check
                    out.push_str(&format!(
                        "\tif {} == nil {{ return nil }}\n",
                        pipe.binding
                    ));
                }
                if i == last {
                    let ret = bu_type_to_go(&output.ty);
                    if !ret.is_empty() {
                        out.push_str(&format!("\treturn {}\n", pipe.binding));
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
                    let ret = bu_type_to_go(&output.ty);
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
        Atom::BuiltinExpr { name, args } => {
            match name.as_str() {
                "assert" => {
                    let cond = emit_expr_go(&args[0]);
                    format!(
                        "func() bool {{ __r := ({cond}); \
                         if !__r {{ fmt.Fprintf(os.Stderr, \"[assert] failed\\n\") }}; \
                         return __r }}()"
                    )
                }
                "assert_eq" => {
                    let lhs = emit_expr_go(&args[0]);
                    let rhs = emit_expr_go(&args[1]);
                    format!(
                        "func() bool {{ __l, __r := ({lhs}), ({rhs}); __ok := __l == __r; \
                         if !__ok {{ fmt.Fprintf(os.Stderr, \
                           \"[assert_eq] expected %v, got %v\\n\", __r, __l) }}; \
                         return __ok }}()"
                    )
                }
                "assert_ne" => {
                    let lhs = emit_expr_go(&args[0]);
                    let rhs = emit_expr_go(&args[1]);
                    format!(
                        "func() bool {{ __l, __r := ({lhs}), ({rhs}); __ok := __l != __r; \
                         if !__ok {{ fmt.Fprintf(os.Stderr, \
                           \"[assert_ne] expected values to differ, both were %v\\n\", __l) }}; \
                         return __ok }}()"
                    )
                }
                other => format!("false /* builtin::{other} not supported as expression */"),
            }
        }
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
                CallArg::Value(s)     => s.clone(),
                CallArg::BulletRef(s) => to_pascal_case(s),
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
        Atom::Closure { params, ret, body } => {
            let ps = params.iter()
                .map(|p| format!("{} {}", p.name, bu_type_to_go(&p.ty)))
                .collect::<Vec<_>>().join(", ");
            let ret_str  = bu_type_to_go(ret);
            let body_str = emit_expr_go(body);
            format!("func({}) {} {{ return {} }}", ps, ret_str, body_str)
        }
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
        BuType::Array(t, n)  => format!("[{}]{}", n, bu_type_to_go(t)),
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
