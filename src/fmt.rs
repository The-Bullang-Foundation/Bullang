//! Bullang AST pretty-printer.
//!
//! Canonical formatting rules:
//!
//! **Inventory files**
//! - Directives always in order: `#rank` → `#lang` → `#lib*` → struct defs → entries
//! - Struct fields left-aligned, colons aligned to the widest field name + 1 space
//! - One blank line between struct definitions
//! - One blank line between struct block and entries block
//! - Entries: `filename : fn1, fn2;` (space either side of colon)
//!
//! **Source files**
//! - One blank line between functions
//! - Pipe bullets indented with 4 spaces
//! - Consistent spacing: `(inputs) : expr -> {binding};`
//! - `#test` annotation on its own line immediately above `let`
//! - Escape block contents are reproduced verbatim — never touched
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
    if let Some(ref lang) = inv.lang {
        out.push_str(&format!("#lang: {};\n", lang.ext()));
    }
    for lib in &inv.libs {
        out.push_str(&format!("#lib: {};\n", lib));
    }

    // Struct definitions
    if !inv.structs.is_empty() {
        out.push('\n');
        for (i, s) in inv.structs.iter().enumerate() {
            if i > 0 { out.push('\n'); }
            out.push_str(&format_struct_def(s));
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

    out.push_str(&format!(
        "let {}{}({}) -> {}: {} {{\n",
        func.name,
        type_param_str,
        params,
        func.output.name,
        format_type(&func.output.ty)
    ));

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
            let mut out = String::new();
            for b in blocks {
                let kw = b.backend.escape_keyword();
                out.push_str(&format!("    @{}\n", kw));
                for line in b.code.lines() {
                    out.push_str(&format!("    {}\n", line));
                }
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
    let inputs = pipe.inputs.join(", ");
    let expr   = format_expr(&pipe.expr);
    let prop   = if pipe.propagate { "?" } else { "" };
    format!("    ({}) : {} -> {{{}}}{};\n", inputs, expr, pipe.binding, prop)
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
        Atom::Float(n) => n.to_string(),
        Atom::Integer(n)       => n.to_string(),
        Atom::StringLit(s)     => format!("\"{}\"", s),
        Atom::Interp(template) => format!("\"{}\"", template),
        Atom::Call { name, args } => {
            let args_str = args.iter().map(|a| match a {
                CallArg::Value(s)     => s.clone(),
                CallArg::BulletRef(s) => format!("&{}", s),
            }).collect::<Vec<_>>().join(", ");
            format!("{}({})", name, args_str)
        }
        Atom::Unary { op, rhs } => format!("({}{})", op, format_atom(rhs)),
        Atom::FieldAccess { base, fields } => format!("{}.{}", base, fields.join(".")),
        Atom::Index { base, idx } =>
            format!("{}[{}]", base, format_expr(idx)),
        Atom::Slice { base, from, to } =>
            format!("{}[{}..{}]", base, format_expr(from), format_expr(to)),
        Atom::BuiltinExpr { name, args } => {
            let args_str = args.iter().map(format_expr).collect::<Vec<_>>().join(", ");
            format!("builtin::{}({})", name, args_str)
        }
        Atom::EnumVariant { ty, variant } => format!("{}.{}", ty, variant),
        Atom::Closure { params, ret, body } => {
            let ps = params.iter()
                .map(|p| format!("{}: {}", p.name, format_type(&p.ty)))
                .collect::<Vec<_>>().join(", ");
            format!("|{}| -> {} {{ {} }}", ps, format_type(ret), format_expr(body))
        }
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
        BuType::Array(t, n)  => format!("[{}; {}]", format_type(t), n),
        BuType::Unknown      => "_".to_string(),
    }
}

