//! Type checking pass.

use std::collections::HashMap;
use std::path::Path;
use std::fs;

use crate::ast::*;
use crate::parser;
use crate::validator::{collect_subdirs, read_inventory};

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct TypeError {
    pub file:    String,
    pub line:    usize,
    pub col:     usize,
    pub message: String,
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line > 0 {
            write!(f, "[{}:{}:{}] {}", self.file, self.line, self.col, self.message)
        } else {
            write!(f, "[{}] {}", self.file, self.message)
        }
    }
}

fn terr(path: &str, span: Span, msg: impl Into<String>) -> TypeError {
    TypeError { file: path.to_string(), line: span.line, col: span.col, message: msg.into() }
}

// ── Public entry points ───────────────────────────────────────────────────────

pub fn typecheck_tree(root: &Path) -> Vec<TypeError> {
    let mut errors = Vec::new();
    check_folder(root, &mut errors);
    errors
}

pub fn typecheck_file(sf: &SourceFile, path: &str) -> Vec<TypeError> {
    let mut errors = Vec::new();
    let struct_env = StructEnv::new();
    let enum_env   = EnumEnv::new();
    for func in &sf.bullets {
        errors.extend(check_function(func, path, &TypeEnv::new(), &struct_env, &enum_env, true));
    }
    errors
}

// ── Folder-level type checking ────────────────────────────────────────────────

fn check_folder(dir: &Path, errors: &mut Vec<TypeError>) -> (TypeEnv, StructEnv, EnumEnv) {
    let inv = match read_inventory(dir) {
        Ok(i)  => i,
        Err(_) => return (TypeEnv::new(), StructEnv::new(), EnumEnv::new()),
    };

    let mut env        = TypeEnv::new();
    let mut struct_env = StructEnv::new();
    let mut enum_env   = EnumEnv::new();

    // Register this folder's own struct and enum definitions.
    for s in &inv.structs {
        struct_env.insert(s.name.clone(), s.clone());
    }
    for e in &inv.enums {
        enum_env.insert(e.name.clone(), e.clone());
    }

    if inv.rank == Rank::War {
        for subdir in collect_subdirs(dir) {
            let (sub_env, sub_struct_env, sub_enum_env) = check_folder(&subdir, errors);
            env.extend(sub_env);
            struct_env.extend(sub_struct_env);
            enum_env.extend(sub_enum_env);
        }
        return (env, struct_env, enum_env);
    }

    if inv.rank.has_sub_folders() {
        for subdir in collect_subdirs(dir) {
            let (sub_env, sub_struct_env, sub_enum_env) = check_folder(&subdir, errors);
            env.extend(sub_env);
            struct_env.extend(sub_struct_env);
            enum_env.extend(sub_enum_env);
        }
    }

    let is_skirmish = inv.rank == Rank::Skirmish;
    for entry in &inv.entries {
        let bu_path  = dir.join(format!("{}.bu", entry.file));
        let file_env = check_source_file(&bu_path, &env, &struct_env, &enum_env, is_skirmish, errors);
        env.extend(file_env);
    }

    (env, struct_env, enum_env)
}

// ── File-level type checking ──────────────────────────────────────────────────

fn check_source_file(
    path:        &Path,
    env:         &TypeEnv,
    struct_env:  &StructEnv,
    enum_env:    &EnumEnv,
    is_skirmish: bool,
    errors:      &mut Vec<TypeError>,
) -> TypeEnv {
    let mut file_env = TypeEnv::new();

    let source = match fs::read_to_string(path) {
        Ok(s)  => s,
        Err(_) => return file_env,
    };

    let mut sf = match parser::parse_file(&source, false) {
        Ok(BuFile::Source(s)) => s,
        _                     => return file_env,
    };

    // Lower FieldAccess nodes that refer to enum names before type-checking.
    crate::ast::lower_enum_refs(&mut sf, enum_env);

    let path_str = path.display().to_string();

    for func in &sf.bullets {
        errors.extend(check_function(func, &path_str, env, struct_env, enum_env, is_skirmish));
        file_env.insert(func.name.clone(), BulletSig {
            params:  func.params.iter().map(|p| p.ty.clone()).collect(),
            returns: func.output.ty.clone(),
        });
    }

    file_env
}

// ── Function-level type checking ──────────────────────────────────────────────

fn check_function(
    func:        &Bullet,
    path:        &str,
    env:         &TypeEnv,
    struct_env:  &StructEnv,
    enum_env:    &EnumEnv,
    is_skirmish: bool,
) -> Vec<TypeError> {
    // Generic functions cannot be type-checked without instantiation.
    // Skip the pipe body — the declaration is still registered in TypeEnv.
    if !func.type_params.is_empty() {
        return vec![];
    }

    let bullets = match &func.body {
        BulletBody::Natives(_)     => return vec![],
        BulletBody::Builtin(_)     => return vec![], // stdlib owns the type contract
        BulletBody::Pipes(p)       => p,
    };

    let mut errors = Vec::new();
    let mut local: HashMap<String, BuType> = func.params.iter()
        .map(|p| (p.name.clone(), p.ty.clone()))
        .collect();

    let last = bullets.len().saturating_sub(1);

    let output_is_propagatable = is_propagatable_type(&func.output.ty);

    for (i, bullet) in bullets.iter().enumerate() {
        if bullet.propagate && !output_is_propagatable {
            errors.push(terr(path, bullet.span, format!(
                "Function '{}': `?` can only be used when the function returns \
                 Option[T] or Result[T, E] (declared return type is {}).",
                func.name, func.output.ty.to_rust()
            )));
        }
        if bullet.propagate && i == last {
            errors.push(terr(path, bullet.span, format!(
                "Function '{}': `?` cannot appear on the last bullet. \
                 Use it on intermediate bullets to propagate None/Err early.",
                func.name
            )));
        }

        let expr_type = infer_expr(
            &bullet.expr, &local, env, struct_env, enum_env, is_skirmish,
            &func.name, path, bullet.span, &mut errors,
        );

        if i == last && !types_compatible(&expr_type, &func.output.ty) {
            errors.push(terr(path, bullet.span, format!(
                "Function '{}': last bullet produces {} but declared output is {}.",
                func.name, expr_type.to_rust(), func.output.ty.to_rust()
            )));
        }

        let binding_ty = if bullet.propagate {
            unwrap_inner_type(&expr_type)
        } else {
            expr_type
        };
        local.insert(bullet.binding.clone(), binding_ty);
    }

    errors
}

// ── Propagation helpers ───────────────────────────────────────────────────────

fn is_propagatable_type(ty: &BuType) -> bool {
    if let BuType::Named(s) = ty {
        s.starts_with("Option[") || s.starts_with("Result[")
    } else {
        false
    }
}

fn unwrap_inner_type(ty: &BuType) -> BuType {
    if let BuType::Named(s) = ty {
        if s.starts_with("Option[") && s.ends_with(']') {
            return BuType::Named(s[7..s.len()-1].trim().to_string());
        }
        if s.starts_with("Result[") && s.ends_with(']') {
            let inner = &s[7..s.len()-1];
            let t = inner.split(',').next().unwrap_or(inner).trim();
            return BuType::Named(t.to_string());
        }
    }
    BuType::Unknown
}

// ── Type inference ────────────────────────────────────────────────────────────

fn infer_expr(
    expr:        &Expr,
    local:       &HashMap<String, BuType>,
    env:         &TypeEnv,
    struct_env:  &StructEnv,
    enum_env:    &EnumEnv,
    is_skirmish: bool,
    func_name:   &str,
    path:        &str,
    span:        Span,
    errors:      &mut Vec<TypeError>,
) -> BuType {
    match expr {
        Expr::Atom(a) => infer_atom(a, local, env, struct_env, enum_env, is_skirmish, func_name, path, span, errors),

        Expr::BinOp(b) => {
            let lhs_ty = infer_atom(&b.lhs, local, env, struct_env, enum_env, is_skirmish, func_name, path, span, errors);
            let rhs_ty = infer_atom(&b.rhs, local, env, struct_env, enum_env, is_skirmish, func_name, path, span, errors);

            if lhs_ty == BuType::Unknown || rhs_ty == BuType::Unknown {
                return BuType::Unknown;
            }

            // Allow String + String as concatenation
            let string_ty = BuType::Named("String".to_string());
            if b.op == "+" && lhs_ty == string_ty && rhs_ty == string_ty {
                return string_ty;
            }

            // Boolean operators: both sides must be bool, result is bool
            let bool_ty = BuType::Named("bool".to_string());
            if b.op == "&&" || b.op == "||" {
                if lhs_ty != bool_ty || rhs_ty != bool_ty {
                    errors.push(terr(path, span, format!(
                        "Function '{}': operator '{}' requires bool on both sides \
                         (left: {}, right: {}).",
                        func_name, b.op, lhs_ty.to_rust(), rhs_ty.to_rust()
                    )));
                    return BuType::Unknown;
                }
                return bool_ty;
            }

            // Comparison operators return bool
            let bool_ty = BuType::Named("bool".to_string());
            let cmp_ops = ["==", "!=", "<", ">", "<=", ">="];
            if cmp_ops.contains(&b.op.as_str()) {
                return bool_ty;
            }

            if lhs_ty != rhs_ty {
                errors.push(terr(path, span, format!(
                    "Function '{}': operator '{}' requires both sides to be the same type \
                     (left: {}, right: {}).",
                    func_name, b.op, lhs_ty.to_rust(), rhs_ty.to_rust()
                )));
                return BuType::Unknown;
            }
            if !lhs_ty.is_numeric() {
                errors.push(terr(path, span, format!(
                    "Function '{}': operator '{}' requires a numeric type, got {}.",
                    func_name, b.op, lhs_ty.to_rust()
                )));
                return BuType::Unknown;
            }
            lhs_ty
        }

        Expr::Tuple(exprs) => {
            BuType::Tuple(exprs.iter().map(|e| {
                infer_expr(e, local, env, struct_env, enum_env, is_skirmish, func_name, path, span, errors)
            }).collect())
        }
    }
}

fn infer_atom(
    atom:        &Atom,
    local:       &HashMap<String, BuType>,
    env:         &TypeEnv,
    struct_env:  &StructEnv,
    enum_env:    &EnumEnv,
    is_skirmish: bool,
    func_name:   &str,
    path:        &str,
    span:        Span,
    errors:      &mut Vec<TypeError>,
) -> BuType {
    match atom {
        Atom::Float(_)    => BuType::Named("f64".to_string()),
        Atom::Integer(_)   => BuType::Unknown,
        Atom::StringLit(_) => BuType::Named("String".to_string()),
        Atom::Interp(_)    => BuType::Named("String".to_string()),
        Atom::Ident(name)  => local.get(name).cloned().unwrap_or(BuType::Unknown),

        Atom::Call { name, args } => {
            if is_skirmish { return BuType::Unknown; }

            let sig = match env.get(name) {
                Some(s) => s.clone(),
                None    => return BuType::Unknown,
            };

            if args.len() != sig.params.len() {
                errors.push(terr(path, span, format!(
                    "Function '{}': '{}' expects {} argument(s) but received {}.",
                    func_name, name, sig.params.len(), args.len()
                )));
                return sig.returns.clone();
            }

            for (i, (arg, expected_ty)) in args.iter().zip(sig.params.iter()).enumerate() {
                match arg {
                    CallArg::Value(v) => {
                        let actual_ty = local.get(v).cloned().unwrap_or(BuType::Unknown);
                        if actual_ty != BuType::Unknown && !types_compatible(&actual_ty, expected_ty) {
                            errors.push(terr(path, span, format!(
                                "Function '{}': argument {} passed to '{}' is {} but {} was expected.",
                                func_name, i + 1, name,
                                actual_ty.to_rust(), expected_ty.to_rust()
                            )));
                        }
                    }
                    CallArg::BulletRef(r) => {
                        let ref_sig = match env.get(r) { Some(s) => s, None => continue };
                        let fn_ty   = build_fn_type(ref_sig);
                        if !types_compatible(&fn_ty, expected_ty) {
                            errors.push(terr(path, span, format!(
                                "Function '{}': '&{}' has type {} but argument {} of '{}' expects {}.",
                                func_name, r,
                                fn_ty.to_rust(), i + 1, name,
                                expected_ty.to_rust()
                            )));
                        }
                    }
                }
            }

            sig.returns.clone()
        }

        Atom::Unary { op, rhs } => {
            let rhs_ty = infer_atom(rhs, local, env, struct_env, enum_env, is_skirmish, func_name, path, span, errors);
            let bool_ty    = BuType::Named("bool".to_string());
            match op.as_str() {
                "!" => {
                    if rhs_ty != BuType::Unknown && rhs_ty != bool_ty {
                        errors.push(terr(path, span, format!(
                            "Function '{}': '!' requires a bool operand, got {}.",
                            func_name, rhs_ty.to_rust()
                        )));
                        return BuType::Unknown;
                    }
                    bool_ty
                }
                "-" => {
                    if rhs_ty != BuType::Unknown && !rhs_ty.is_numeric() {
                        errors.push(terr(path, span, format!(
                            "Function '{}': unary '-' requires a numeric operand, got {}.",
                            func_name, rhs_ty.to_rust()
                        )));
                        return BuType::Unknown;
                    }
                    rhs_ty
                }
                other => {
                    errors.push(terr(path, span, format!(
                        "Function '{}': unknown unary operator '{}'.", func_name, other
                    )));
                    BuType::Unknown
                }
            }
        }

        Atom::FieldAccess { base, fields } => {
            // Start from the type of the base variable in the local scope.
            let base_ty = local.get(base).cloned().unwrap_or(BuType::Unknown);
            let mut current = base_ty;

            for field in fields {
                match &current.clone() {
                    BuType::Unknown => return BuType::Unknown,
                    BuType::Named(struct_name) => {
                        match struct_env.get(struct_name) {
                            Some(def) => {
                                match def.fields.iter().find(|f| &f.name == field) {
                                    Some(f) => current = f.ty.clone(),
                                    None => {
                                        errors.push(terr(path, span, format!(
                                            "Function '{}': struct '{}' has no field '{}'.",
                                            func_name, struct_name, field
                                        )));
                                        return BuType::Unknown;
                                    }
                                }
                            }
                            // Struct may come from a rank not yet visible; skip silently.
                            None => return BuType::Unknown,
                        }
                    }
                    other => {
                        errors.push(terr(path, span, format!(
                            "Function '{}': cannot access field '{}' on non-struct type {}.",
                            func_name, field, other.to_rust()
                        )));
                        return BuType::Unknown;
                    }
                }
            }
            current
        }

        Atom::BuiltinExpr { name, args } => {
            let bool_ty = BuType::Named("bool".to_string());
            match name.as_str() {
                "assert" => {
                    if args.len() != 1 {
                        errors.push(terr(path, span, format!(
                            "Function '{}': builtin::assert expects 1 argument, got {}.",
                            func_name, args.len()
                        )));
                        return BuType::Unknown;
                    }
                    let cond_ty = infer_expr(&args[0], local, env, struct_env, enum_env, is_skirmish, func_name, path, span, errors);
                    if cond_ty != BuType::Unknown && cond_ty != bool_ty {
                        errors.push(terr(path, span, format!(
                            "Function '{}': builtin::assert requires a bool argument, got {}.",
                            func_name, cond_ty.to_rust()
                        )));
                    }
                    bool_ty
                }
                "assert_eq" | "assert_ne" => {
                    if args.len() != 2 {
                        errors.push(terr(path, span, format!(
                            "Function '{}': builtin::{} expects 2 arguments, got {}.",
                            func_name, name, args.len()
                        )));
                        return BuType::Unknown;
                    }
                    bool_ty
                }
                other => {
                    errors.push(terr(path, span, format!(
                        "Function '{}': 'builtin::{}' is not available as a pipe expression. \
                         Use it as a function body or check `bullang stdlib --list`.",
                        func_name, other
                    )));
                    BuType::Unknown
                }
            }
        }

        Atom::EnumVariant { ty, variant } => {
            match enum_env.get(ty) {
                Some(def) => {
                    if def.variants.iter().any(|v| &v.name == variant) {
                        BuType::Named(ty.clone())
                    } else {
                        errors.push(terr(path, span, format!(
                            "Function '{}': enum '{}' has no variant '{}'.",
                            func_name, ty, variant
                        )));
                        BuType::Unknown
                    }
                }
                None => {
                    errors.push(terr(path, span, format!(
                        "Function '{}': '{}' is not a known enum type.",
                        func_name, ty
                    )));
                    BuType::Unknown
                }
            }
        }

        Atom::Index { base, .. } => {
            let base_ty = local.get(base).cloned().unwrap_or(BuType::Unknown);
            let string_ty = BuType::Named("String".to_string());
            if base_ty != BuType::Unknown && base_ty != string_ty {
                errors.push(terr(path, span, format!(
                    "Function '{}': index operator [] requires a String, got {}.",
                    func_name, base_ty.to_rust()
                )));
                return BuType::Unknown;
            }
            BuType::Named("char".to_string())
        }

        Atom::Slice { base, .. } => {
            let base_ty = local.get(base).cloned().unwrap_or(BuType::Unknown);
            let string_ty = BuType::Named("String".to_string());
            if base_ty != BuType::Unknown && base_ty != string_ty {
                errors.push(terr(path, span, format!(
                    "Function '{}': slice operator [..] requires a String, got {}.",
                    func_name, base_ty.to_rust()
                )));
                return BuType::Unknown;
            }
            string_ty
        }

        Atom::Closure { params, ret, .. } => {
            // Construct a Fn[T, U -> V] type matching existing representation.
            let param_tys = params.iter().map(|p| p.ty.to_rust()).collect::<Vec<_>>().join(", ");
            let fn_str = if params.is_empty() {
                format!("Fn[-> {}]", ret.to_rust())
            } else {
                format!("Fn[{} -> {}]", param_tys, ret.to_rust())
            };
            BuType::Named(fn_str)
        }
    }
}

// ── Type utilities ────────────────────────────────────────────────────────────

fn build_fn_type(sig: &BulletSig) -> BuType {
    let params = sig.params.iter().map(|t| t.to_rust()).collect::<Vec<_>>().join(", ");
    let ret = sig.returns.to_rust();
    let fn_str = if params.is_empty() { format!("Fn[-> {}]", ret) }
                 else { format!("Fn[{} -> {}]", params, ret) };
    BuType::Named(fn_str)
}

fn normalize(s: &str) -> String { s.split_whitespace().collect() }

fn types_compatible(a: &BuType, b: &BuType) -> bool {
    if a == &BuType::Unknown || b == &BuType::Unknown { return true; }
    match (a, b) {
        (BuType::Named(sa), BuType::Named(sb)) => normalize(sa) == normalize(sb),
        _ => a == b,
    }
}
