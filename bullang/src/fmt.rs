//! Bullang AST pretty-printer.
//!
//! Canonical formatting rules:
//!
//! **Inventory files**
//! - Directives always in order: `#rank` → `#lang` → `#lib*` → `#use*`
//! - Then struct definitions, enum definitions, native blocks, then entries
//! - Struct fields left-aligned, colons aligned to the widest field name + 1 space
//! - One blank line between struct definitions
//! - One blank line between struct block and entries block
//! - Entries: `filename : fn1, fn2;` (space either side of colon)
//!
//! **Source files**
//! - One blank line between functions
//! - Pipe bullets indented with 4 spaces
//! - Consistent spacing: `(inputs) : expr -> {binding};`
//! - Escape block contents are reproduced verbatim — byte for byte, including
//!   their original indentation. An escape block is a macro: Bullang copies it
//!   into the generated file untouched.
//! - Builtin call reproduced verbatim

use crate::ast::*;

// ── Public entry points ───────────────────────────────────────────────────────

/// Format a parsed source file to canonical Bullang style.
pub fn format_source(sf: &SourceFile) -> String {
    let mut out = String::new();
    for (i, func) in sf.bullets.iter().enumerate() {
        if i > 0 { out.push('\n'); }
        out.push_str(&format_bullet(func));
    }
    out
}

/// Format a parsed inventory file to canonical Bullang style.
pub fn format_inventory(inv: &InventoryFile) -> String {
    let mut out = String::new();

    // Directives
    out.push_str(&format!("#rank: {};\n", inv.rank.name()));
    if let Some(ext) = inv.lang.as_ref().and_then(|l| l.ext()) {
        out.push_str(&format!("#lang: {};\n", ext));
    }
    for lib in &inv.libs {
        out.push_str(&format!("#lib: {};\n", lib));
    }
    for use_ in &inv.uses {
        out.push_str(&format!("#use: {};\n", use_));
    }

    // Struct definitions
    if !inv.structs.is_empty() {
        out.push('\n');
        for (i, s) in inv.structs.iter().enumerate() {
            if i > 0 { out.push('\n'); }
            out.push_str(&format_struct_def(s));
        }
    }

    // Enum definitions. These were previously dropped on the floor here, which
    // meant `bullarchy fmt` silently deleted every enum in the file — and
    // `check`'s format pass then reported the file as unformatted forever.
    if !inv.enums.is_empty() {
        out.push('\n');
        for (i, e) in inv.enums.iter().enumerate() {
            if i > 0 { out.push('\n'); }
            out.push_str(&format_enum_def(e));
        }
    }

    // Native blocks, reproduced exactly as written.
    if !inv.natives.is_empty() {
        out.push('\n');
        for nb in &inv.natives {
            out.push_str(&format_native_block(nb));
        }
    }

    // Inventory entries
    if !inv.entries.is_empty() {
        out.push('\n');
        for entry in &inv.entries {
            out.push_str(&format_inv_entry(entry));
        }
    }

    out
}

// ── Struct formatting ─────────────────────────────────────────────────────────

fn format_struct_def(s: &StructDef) -> String {
    let mut out = String::new();
    out.push_str(&format!("struct {} {{\n", s.name));

    // Align colons: pad field names to the width of the longest one
    let max_name = s.fields.iter().map(|f| f.name.len()).max().unwrap_or(0);
    for field in &s.fields {
        let padding = max_name - field.name.len();
        out.push_str(&format!(
            "    {}{} : {},\n",
            field.name,
            " ".repeat(padding),
            format_type(&field.ty)
        ));
    }

    out.push_str("}\n");
    out
}

fn format_enum_def(e: &EnumDef) -> String {
    let mut out = String::new();
    out.push_str(&format!("enum {} {{\n", e.name));
    for v in &e.variants {
        out.push_str(&format!("    {},\n", v.name));
    }
    out.push_str("}\n");
    out
}

/// An escape block is reproduced byte for byte between its delimiters.
fn format_native_block(nb: &NativeBlock) -> String {
    format!("@{}\n{}@end\n", nb.backend.escape_keyword(), nb.code)
}

// ── Inventory entry formatting ────────────────────────────────────────────────

fn format_inv_entry(entry: &InventoryEntry) -> String {
    format!("{} : {};\n", entry.file, entry.functions.join(", "))
}

// ── Bullet (function) formatting ──────────────────────────────────────────────

fn format_bullet(func: &Bullet) -> String {
    let mut out = String::new();

    let params = func.params.iter()
        .map(|p| format!("{}: {}", p.name, format_type(&p.ty)))
        .collect::<Vec<_>>()
        .join(", ");

    let type_param_str = if func.type_params.is_empty() {
        String::new()
    } else {
        format!("[{}]", func.type_params.join(", "))
    };

    let sig = match &func.output {
        Some(o) => format!("let {}{}({}) -> {}: {} {{\n",
            func.name, type_param_str, params, o.name, format_type(&o.ty)),
        None    => format!("let {}{}({}) {{\n",
            func.name, type_param_str, params),
    };
    out.push_str(&sig);

    out.push_str(&format_body(&func.body));
    out.push_str("}\n");
    out
}

// ── Bullet body formatting ────────────────────────────────────────────────────

fn format_body(body: &BulletBody) -> String {
    match body {
        BulletBody::Pipes(pipes) => {
            pipes.iter().map(format_pipe).collect()
        }
        BulletBody::Natives(blocks) => {
            // Verbatim: the contents are the author's code in another language.
            // Bullang does not reindent it, because any alignment it chose
            // would be a guess about a language it does not parse.
            let mut out = String::new();
            for b in blocks {
                out.push_str(&format!("    @{}\n", b.backend.escape_keyword()));
                out.push_str(&b.code);
                if !b.code.ends_with('\n') { out.push('\n'); }
                out.push_str("    @end\n");
            }
            out
        }
        BulletBody::Builtin(name) => {
            format!("    builtin::{}\n", name)
        }
    }
}

// ── Pipe formatting ───────────────────────────────────────────────────────────

fn format_pipe(pipe: &Pipe) -> String {
    let inputs = pipe.inputs.iter().map(|e| format_expr(e)).collect::<Vec<_>>().join(", ");
    let expr   = format_expr(&pipe.expr);
    let bind   = pipe.binding.as_deref().unwrap_or("");
    format!("    ({}) : {} -> {{{}}};\n", inputs, expr, bind)
}

// ── Expression formatting ─────────────────────────────────────────────────────

fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::Atom(a)      => format_atom(a),
        Expr::BinOp(b)     => format!("{} {} {}", format_atom(&b.lhs), b.op, format_atom(&b.rhs)),
        Expr::Tuple(exprs) => format!(
            "({})", exprs.iter().map(format_expr).collect::<Vec<_>>().join(", ")
        ),
    }
}

fn format_atom(atom: &Atom) -> String {
    match atom {
        Atom::Ident(s)         => s.clone(),
        // A float must still read as a float after a round trip: Rust renders
        // 2.0 as "2", which would re-parse as an integer and silently change
        // the type.
        Atom::Float(n) => format_float(*n),
        Atom::Integer(n)       => n.to_string(),
        Atom::StringLit(s)     => format!("\"{}\"", s),
        Atom::Interp(template) => format!("\"{}\"", template),
        Atom::Call { name, args } => {
            let args_str = args.iter().map(|a| match a {
                CallArg::Value(s) => s.clone(),
            }).collect::<Vec<_>>().join(", ");
            format!("{}({})", name, args_str)
        }
        // No parentheses: `atom` has no parenthesised alternative, so "(!a)"
        // would not parse back.
        Atom::Unary { op, rhs } => format!("{}{}", op, format_atom(rhs)),
        Atom::FieldAccess { base, fields } => format!("{}.{}", base, fields.join(".")),
        Atom::Index { base, idx } =>
            format!("{}[{}]", base, format_expr(idx)),
        Atom::Slice { base, from, to } =>
            format!("{}[{}..{}]", base, format_expr(from), format_expr(to)),
        Atom::BuiltinExpr { name, args } => {
            let args_str = args.iter().map(format_expr).collect::<Vec<_>>().join(", ");
            format!("builtin::{}({})", name, args_str)
        }
        Atom::BuiltinNoArgs(name)         => format!("builtin::{}", name),
        Atom::EnumVariant { ty, variant } => format!("{}.{}", ty, variant),
    }
}

/// Render a float so it always reads back as a float.
fn format_float(n: f64) -> String {
    let s = n.to_string();
    if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") {
        s
    } else {
        format!("{}.0", s)
    }
}

// ── Type formatting ───────────────────────────────────────────────────────────

pub fn format_type(ty: &BuType) -> String {
    match ty {
        BuType::Named(s)     => s.clone(),
        BuType::Tuple(inner) => format!(
            "Tuple[{}]",
            inner.iter().map(format_type).collect::<Vec<_>>().join(", ")
        ),
        BuType::Unknown      => "_".to_string(),
    }
}

