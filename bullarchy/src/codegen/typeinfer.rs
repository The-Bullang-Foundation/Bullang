//! Minimal, best-effort local type tracking for builtin-call emission.
//!
//! The Bullang AST (`Pipe`) carries no type annotations at all — bindings
//! are just an `Option<String>` name, nothing more (confirmed against the
//! `bullang` crate's `ast.rs` directly). Historically every builtin-call
//! site hardcoded `Param { ty: BuType::Unknown, .. }` when constructing the
//! synthetic params passed into `stdlib::emit_builtin`, so a builtin that
//! genuinely needs to know its own argument types (e.g. `swap` needing to
//! construct a concrete `Tuple[T,T]` value) had no way to do so — which is
//! exactly how `swap`'s C and Go emission ended up broken.
//!
//! It reaches all six backends. It used to be wired into C and Go only, so a
//! builtin needing its argument types worked there and silently degraded to
//! `Unknown` everywhere else — the same program emitting different code
//! depending on the target, for no reason a reader could see.
//!
//! This is intentionally NOT a full type-checker: it only resolves the
//! cases needed to unblock type-dependent builtin emission — identifiers
//! already in scope (the enclosing function's own params, or an earlier
//! same-file pipe binding), calls to other same-file functions (via their
//! declared output type), and literals. Anything else — and any case a
//! future maintainer didn't anticipate — resolves to `BuType::Unknown`.
//! Callers that need a concrete type should treat `Unknown` as "couldn't
//! infer" and fail loudly (return an `Err` from `emit()`, surfaced as a
//! visible `/* ERROR */` comment) rather than guess and silently emit the
//! wrong shape — that silent failure is exactly the bug class this exists
//! to close off.

use bullang::ast::*;
use std::collections::HashMap;

/// Declared output type of every function in `file`, keyed by name.
///
/// Same-file only — mirrors the existing same-file limitation already
/// documented on `collect_unit_functions` elsewhere in codegen_c.rs. In a
/// multi-file project build, a caller in `main.bu` invoking a function
/// declared in a different `.bu` module won't have its type resolved by
/// this (falls back to `Unknown`) — that cross-file case isn't covered
/// here either, consistent with the rest of this codebase's current scope.
pub fn collect_fn_output_types(file: &SourceFile) -> HashMap<String, BuType> {
    file.bullets.iter()
        .map(|f| (
            f.name.clone(),
            f.output.as_ref().map(|o| o.ty.clone())
                .unwrap_or_else(|| BuType::unit()),
        ))
        .collect()
}

/// Tracks locally-known variable types while walking a function's `Pipes`
/// body in declaration order, so builtin-call emission can ask "what type
/// is this argument" instead of assuming `Unknown`.
pub struct TypeEnv<'a> {
    vars: HashMap<String, BuType>,
    fn_outputs: &'a HashMap<String, BuType>,
}

impl<'a> TypeEnv<'a> {
    /// Seed from the enclosing function's own declared parameters — the
    /// one place types are always known for certain, since `Param` (unlike
    /// `Pipe`) does carry a real `ty: BuType` field.
    pub fn seed(params: &[Param], fn_outputs: &'a HashMap<String, BuType>) -> Self {
        let vars = params.iter().map(|p| (p.name.clone(), p.ty.clone())).collect();
        TypeEnv { vars, fn_outputs }
    }

    /// Best-effort type of `expr` given what's known so far. `Unknown`
    /// means "couldn't resolve", not "resolved to some universal type" —
    /// treat it as a hard "don't know", never as a stand-in for a real type.
    pub fn infer(&self, expr: &Expr) -> BuType {
        match expr {
            Expr::Atom(a)  => self.infer_atom(a),
            Expr::BinOp(b) => self.infer_binop(b),
            // A tuple's type is its parts', if all of them are known.
            Expr::Tuple(items) => {
                let mut parts = Vec::with_capacity(items.len());
                for item in items {
                    match self.infer(item) {
                        BuType::Unknown => return BuType::Unknown,
                        ty              => parts.push(ty),
                    }
                }
                BuType::Tuple(parts)
            }
        }
    }

    fn infer_atom(&self, atom: &Atom) -> BuType {
        match atom {
            Atom::Ident(name)  => self.vars.get(name).cloned().unwrap_or(BuType::Unknown),
            Atom::Call { name, .. } =>
                self.fn_outputs.get(name).cloned().unwrap_or(BuType::Unknown),
            Atom::Integer(_)   => BuType::Named("i64".to_string()),
            Atom::Float(_)     => BuType::Named("f64".to_string()),
            Atom::StringLit(_) | Atom::Interp(_) => BuType::Named("String".to_string()),
            // A builtin's type comes from the catalogue, which is the same
            // source the type checker uses — so codegen and `check` agree on
            // what a builtin produces instead of codegen giving up.
            Atom::BuiltinExpr { name, args } => self.infer_builtin(name, args),
            Atom::BuiltinNoArgs(name)        => self.infer_builtin(name, &[]),
            Atom::FieldAccess { .. } | Atom::Index { .. } => BuType::Unknown,
            Atom::Slice { .. }    => BuType::Named("String".to_string()),
            Atom::Unary { rhs, .. } => self.infer_atom(rhs),
            Atom::EnumVariant { ty, .. } => BuType::Named(ty.clone()),
        }
    }

    /// A comparison is a bool; any other operator with matching operands has
    /// their type.
    fn infer_binop(&self, b: &BinExpr) -> BuType {
        if matches!(b.op.as_str(), "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||") {
            return BuType::Named("bool".to_string());
        }
        let lhs = self.infer_atom(&b.lhs);
        let rhs = self.infer_atom(&b.rhs);
        match (&lhs, &rhs) {
            (BuType::Unknown, other) | (other, BuType::Unknown) => other.clone(),
            _ if lhs == rhs => lhs,
            // Mismatched operands are the type checker's to report, not
            // something to guess a type for.
            _ => BuType::Unknown,
        }
    }

    fn infer_builtin(&self, name: &str, args: &[Expr]) -> BuType {
        let Some(builtin) = bullang::stdlib::find(name) else {
            return BuType::Unknown;
        };
        // `Ty::Same` resolves to whatever the caller passed in the builtin's
        // interchangeable positions — the same rule the type checker applies.
        let same = builtin.params.iter().zip(args)
            .find(|(p, _)| p.to_butype().is_none())
            .map(|(_, arg)| self.infer(arg))
            .filter(|ty| *ty != BuType::Unknown);
        builtin.returns.resolve(same.as_ref()).unwrap_or(BuType::Unknown)
    }

    /// Record `name`'s inferred type after a pipe binds it.
    pub fn bind(&mut self, name: &str, ty: BuType) {
        self.vars.insert(name.to_string(), ty);
    }
}
