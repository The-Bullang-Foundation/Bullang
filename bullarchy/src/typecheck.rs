//! Type checking pass.

use std::collections::HashMap;
use std::path::Path;

use bullang::ast::*;
use bullang::parser;
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

    // `main.bu` is not an inventory entry — it is found by name — so iterating
    // `inv.entries` skipped the one file every project has and most of the
    // program's top-level logic lives in. It was never type-checked at all.
    if let Some(main_path) = crate::validator::helpers::main_bu_path(dir) {
        let file_env = check_source_file(&main_path, &env, &struct_env, &enum_env, is_skirmish, errors);
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

    let source = match crate::overlay::read_source(path) {
        Ok(s) => s,
        Err(e) => {
            errors.push(TypeError {
                file: path.display().to_string(),
                line: 0, col: 0,
                message: format!("Could not read: {}", e),
            });
            return file_env;
        }
    };

    // The tolerant parser, which is what the validator uses. The strict one
    // here meant a file the validator had already reported on was silently
    // skipped by the type checker — so a project could report parse errors and
    // no type errors, and look as though only its syntax was wrong. Two parse
    // strategies also meant the two passes could disagree about what a file
    // even contained.
    let parsed = parser::parse_file_tolerant(&source, &path.display().to_string());
    let mut sf = match parsed.file {
        BuFile::Source(s) => s,
        // The validator reports the parse errors themselves; repeating them
        // here would print each one twice. What the tolerant parser did
        // recover is still checked.
        BuFile::Inventory(_) => return file_env,
    };

    // Lower FieldAccess nodes that refer to enum names before type-checking.
    bullang::ast::lower_enum_refs(&mut sf, enum_env);

    let path_str = path.display().to_string();

    for func in &sf.bullets {
        errors.extend(check_function(func, &path_str, env, struct_env, enum_env, is_skirmish));
        file_env.insert(func.name.clone(), BulletSig {
            params:  func.params.iter().map(|p| p.ty.clone()).collect(),
            returns: func.output.as_ref().map(|o| o.ty.clone()).unwrap_or(BuType::unit()),
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

    let unit_ty = BuType::unit();
    let output_ty = func.output.as_ref().map(|o| &o.ty).unwrap_or(&unit_ty);
    for (i, bullet) in bullets.iter().enumerate() {

        let expr_type = infer_pipe(
            bullet, &local, env, struct_env, enum_env, is_skirmish,
            &func.name, path, &mut errors,
        );

        // A discarding last bullet (`-> {}`) produces nothing for the caller,
        // so its expression's type is not the function's return type — the
        // same distinction the validator draws in decision 8. Comparing them
        // made every `main` that ended in a builtin returning a value (which
        // is most of them: `builtin::out` returns a byte count) report a
        // mismatch against `()`.
        let discards_last = bullet.binding.as_deref().is_none_or(|b| b.is_empty() || b == "_");
        if i == last && !discards_last && !types_compatible(&expr_type, output_ty) {
            errors.push(terr(path, bullet.span, format!(
                "Function '{}': last bullet produces {} but declared output is {}.",
                func.name, expr_type.display(), output_ty.display()
            )));
        }
        if i == last && discards_last && !output_ty.is_unit() {
            errors.push(terr(path, bullet.span, format!(
                "Function '{}': last bullet discards its result, but the function \
                 declares an output of type {}.",
                func.name, output_ty.display()
            )));
        }

        if let Some(ref name) = bullet.binding {
            local.insert(name.clone(), expr_type);
        }
    }

    errors
}

// ── Propagation helpers ───────────────────────────────────────────────────────

// ── Pipe inference ────────────────────────────────────────────────────────────

/// The type a bullet's right-hand side produces.
///
/// This exists because a pipe's arguments live in `pipe.inputs`, not inside
/// `pipe.expr`, so inference has to start here rather than at the expression.
/// `typecheck.rs:437`'s `unreachable!()` was exactly that gap: `infer_atom`
/// received a bare `builtin::name` with no way to reach the values it was
/// applied to, and crashed `check` and `convert` on every project that used
/// one.
fn infer_pipe(
    pipe:        &Pipe,
    local:       &HashMap<String, BuType>,
    env:         &TypeEnv,
    struct_env:  &StructEnv,
    enum_env:    &EnumEnv,
    is_skirmish: bool,
    func_name:   &str,
    path:        &str,
    errors:      &mut Vec<TypeError>,
) -> BuType {
    let span = pipe.span;
    match crate::pipe::classify(pipe) {
        crate::pipe::PipeRhs::Expr(expr) => infer_expr(
            expr, local, env, struct_env, enum_env, is_skirmish,
            func_name, path, span, errors,
        ),

        crate::pipe::PipeRhs::Call { name, args } => {
            if is_skirmish { return BuType::Unknown; }
            let sig = match env.get(name) {
                Some(sig) => sig,
                None => return BuType::Unknown,
            };
            if sig.params.len() != args.len() {
                errors.push(terr(path, span, format!(
                    "Function '{}': '{}' takes {} argument(s) but this bullet supplies {}.",
                    func_name, name, sig.params.len(), args.len()
                )));
                return sig.returns.clone();
            }
            for (i, (arg, expected)) in args.iter().zip(sig.params.iter()).enumerate() {
                let actual = infer_expr(
                    arg, local, env, struct_env, enum_env, is_skirmish,
                    func_name, path, span, errors,
                );
                if actual != BuType::Unknown && !types_compatible(&actual, expected) {
                    errors.push(terr(path, span, format!(
                        "Function '{}': argument {} passed to '{}' is {} but {} was expected.",
                        func_name, i + 1, name, actual.display(), expected.display()
                    )));
                }
            }
            sig.returns.clone()
        }

        crate::pipe::PipeRhs::Builtin { name, args } => infer_builtin(
            name, args, local, env, struct_env, enum_env, is_skirmish,
            func_name, path, span, errors,
        ),
    }
}

/// A builtin's type, from the catalogue.
///
/// The catalogue carries machine-readable `params` and `returns` — that is
/// what makes this possible at all; the signature used to be a string written
/// for a reader, and nothing could type-check against it.
///
/// A builtin absent from the catalogue is not an error here. Package builtins
/// arrive through `#use:` and Bullarchy resolves those against whatever the
/// project has installed, so an unknown name is left unconstrained and caught
/// at emission instead.
fn infer_builtin(
    name:        &str,
    args:        &[Expr],
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
    let builtin = match bullang::stdlib::find(name) {
        Some(b) => b,
        None => {
            if !crate::stdlib::is_known_builtin(name) {
                errors.push(terr(path, span, format!(
                    "Function '{}': 'builtin::{}' is not a known builtin. Run \
                     `bullang stdlib` to see the core set, or install the package \
                     that provides it with `bullarchy add`.",
                    func_name, name
                )));
            }
            return BuType::Unknown;
        }
    };

    if builtin.params.len() != args.len() {
        errors.push(terr(path, span, format!(
            "Function '{}': 'builtin::{}' takes {} argument(s) but this bullet \
             supplies {}. Its signature is `{}`.",
            func_name, name, builtin.params.len(), args.len(), builtin.signature
        )));
        return builtin.returns.to_butype().unwrap_or(BuType::Unknown);
    }

    // `Ty::Same` means "whatever the caller passed", and every Same position
    // in one call must agree. The first argument whose type is known fixes it.
    let mut same: Option<BuType> = None;

    for (i, (arg, expected)) in args.iter().zip(builtin.params.iter()).enumerate() {
        let actual = infer_expr(
            arg, local, env, struct_env, enum_env, is_skirmish,
            func_name, path, span, errors,
        );
        if actual == BuType::Unknown {
            continue;
        }
        match expected.to_butype() {
            Some(want) => {
                if !types_compatible(&actual, &want) {
                    errors.push(terr(path, span, format!(
                        "Function '{}': argument {} of 'builtin::{}' is {} but {} \
                         was expected.",
                        func_name, i + 1, name, actual.display(), want.display()
                    )));
                }
            }
            None => match &same {
                None => same = Some(actual),
                Some(fixed) if !types_compatible(&actual, fixed) => {
                    errors.push(terr(path, span, format!(
                        "Function '{}': 'builtin::{}' requires its interchangeable \
                         arguments to have one type, but argument {} is {} where {} \
                         was already established.",
                        func_name, name, i + 1, actual.display(), fixed.display()
                    )));
                }
                Some(_) => {}
            },
        }
    }
    // `Same` in the return type — bare, or inside a tuple as `swap`'s
    // `Tuple[T, T]` — stands for whatever the arguments fixed.
    builtin.returns.resolve(same.as_ref()).unwrap_or(BuType::Unknown)
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
                        func_name, b.op, lhs_ty.display(), rhs_ty.display()
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
                    func_name, b.op, lhs_ty.display(), rhs_ty.display()
                )));
                return BuType::Unknown;
            }
            if !lhs_ty.is_numeric() {
                errors.push(terr(path, span, format!(
                    "Function '{}': operator '{}' requires a numeric type, got {}.",
                    func_name, b.op, lhs_ty.display()
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
                {
                    let CallArg::Value(v) = arg;
                    {
                        let actual_ty = local.get(v).cloned().unwrap_or(BuType::Unknown);
                        if actual_ty != BuType::Unknown && !types_compatible(&actual_ty, expected_ty) {
                            errors.push(terr(path, span, format!(
                                "Function '{}': argument {} passed to '{}' is {} but {} was expected.",
                                func_name, i + 1, name,
                                actual_ty.display(), expected_ty.display()
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
                            func_name, rhs_ty.display()
                        )));
                        return BuType::Unknown;
                    }
                    bool_ty
                }
                "-" => {
                    if rhs_ty != BuType::Unknown && !rhs_ty.is_numeric() {
                        errors.push(terr(path, span, format!(
                            "Function '{}': unary '-' requires a numeric operand, got {}.",
                            func_name, rhs_ty.display()
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
                            func_name, field, other.display()
                        )));
                        return BuType::Unknown;
                    }
                }
            }
            current
        }

        // A bare builtin only ever appears as a pipe's expression, where its
        // arguments are the pipe's inputs — which this function cannot see.
        // `infer_pipe` classifies that case before it gets here, so reaching
        // this arm means the atom appeared somewhere a builtin cannot go.
        Atom::BuiltinNoArgs(name) => {
            errors.push(terr(path, span, format!(
                "Function '{}': 'builtin::{}' needs its arguments. Write it as a \
                 bullet — `(args) : builtin::{} -> {{result}};` — or call it \
                 inline as `builtin::{}(args)`.",
                func_name, name, name, name
            )));
            BuType::Unknown
        }
        // An inline builtin is the same builtin, typed the same way — the
        // three `assert` special cases that used to live here were not in the
        // catalogue and were emitted by nothing.
        Atom::BuiltinExpr { name, args } => infer_builtin(
            name, args, local, env, struct_env, enum_env, is_skirmish,
            func_name, path, span, errors,
        ),

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
                    func_name, base_ty.display()
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
                    func_name, base_ty.display()
                )));
                return BuType::Unknown;
            }
            string_ty
        }

    }
}

// ── Type utilities ────────────────────────────────────────────────────────────

fn normalize(s: &str) -> String { s.split_whitespace().collect() }

fn types_compatible(a: &BuType, b: &BuType) -> bool {
    if a == &BuType::Unknown || b == &BuType::Unknown { return true; }
    match (a, b) {
        (BuType::Named(sa), BuType::Named(sb)) => normalize(sa) == normalize(sb),
        _ => a == b,
    }
}
