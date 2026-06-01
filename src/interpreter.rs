//! Bullang tree-walk interpreter.
//!
//! Executes pure Bullang source files directly — no transpilation, no native
//! escape blocks. Entry point is `run`, which looks for a zero-argument `main`
//! bullet and evaluates it.
//!
//! Supported:
//!   - All arithmetic and comparison operators
//!   - String interpolation, indexing, and slicing
//!   - Function calls between bullets in the same source file
//!   - The full stdlib including open/close (fd table), sorting, env, swap
//!
//! Not supported (clear runtime error):
//!   - Native escape blocks — use `bullarchy convert` instead
//!   - Closures
//!   - Struct field access
//!   - Bullet references as call arguments

use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::Mutex;
use crate::ast::{Atom, BinExpr, BulletBody, CallArg, Expr, Pipe, SourceFile};

// ── File descriptor table ─────────────────────────────────────────────────────
// fd 0 = stdin, 1 = stdout, 2 = stderr are implicit.
// open() allocates from 3 upward and stores the file handle here.

struct FdTable {
    next: i64,
    files: HashMap<i64, std::fs::File>,
}

impl FdTable {
    fn new() -> Self {
        Self { next: 3, files: HashMap::new() }
    }

    fn open(&mut self, path: &str, mode: &str) -> Result<i64, InterpError> {
        use std::fs::OpenOptions;
        let file = match mode {
            "r"  => OpenOptions::new().read(true).open(path),
            "w"  => OpenOptions::new().write(true).create(true).truncate(true).open(path),
            "a"  => OpenOptions::new().append(true).create(true).open(path),
            "rw" => OpenOptions::new().read(true).write(true).create(true).open(path),
            m    => return Err(InterpError::new(format!(
                "open: unknown mode '{}' — use 'r', 'w', 'a', or 'rw'", m
            ))),
        }.map_err(|e| InterpError::new(format!("open: {}", e)))?;

        let fd = self.next;
        self.next += 1;
        self.files.insert(fd, file);
        Ok(fd)
    }

    fn close(&mut self, fd: i64) -> Result<(), InterpError> {
        if self.files.remove(&fd).is_some() {
            Ok(())
        } else {
            Err(InterpError::new(format!("close: fd {} is not open", fd)))
        }
    }
}

static FD_TABLE: Mutex<Option<FdTable>> = Mutex::new(None);

fn with_fd_table<F, T>(f: F) -> T
where F: FnOnce(&mut FdTable) -> T {
    let mut guard = FD_TABLE.lock().unwrap();
    if guard.is_none() {
        *guard = Some(FdTable::new());
    }
    f(guard.as_mut().unwrap())
}

// ── Values ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Array(Vec<Value>),
    Tuple(Vec<Value>),
    Unit,
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(n)    => write!(f, "{}", n),
            Value::Float(n)  => write!(f, "{}", n),
            Value::Bool(b)   => write!(f, "{}", b),
            Value::Str(s)    => write!(f, "{}", s),
            Value::Unit      => Ok(()),
            Value::Array(vs) => {
                write!(f, "[")?;
                for (i, v) in vs.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            Value::Tuple(vs) => {
                write!(f, "(")?;
                for (i, v) in vs.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", v)?;
                }
                write!(f, ")")
            }
        }
    }
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct InterpError {
    pub message: String,
    pub bullet:  Option<String>,
}

impl InterpError {
    fn new(msg: impl Into<String>) -> Self {
        Self { message: msg.into(), bullet: None }
    }

    fn in_bullet(mut self, name: &str) -> Self {
        self.bullet = Some(name.to_string());
        self
    }
}

impl std::fmt::Display for InterpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.bullet {
            Some(b) => write!(f, "in '{}': {}", b, self.message),
            None    => write!(f, "{}", self.message),
        }
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Execute a source file. Looks for a zero-argument `main` bullet.

// ── String escape sequences ───────────────────────────────────────────────────

fn unescape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n')  => result.push('\n'),
                Some('t')  => result.push('\t'),
                Some('r')  => result.push('\r'),
                Some('\\') => result.push('\\'),
                Some('"')  => result.push('"'),
                Some(c)    => { result.push('\\'); result.push(c); }
                None       => result.push('\\'),
            }
        } else {
            result.push(ch);
        }
    }
    result
}

pub fn run(source: &SourceFile) -> Result<(), InterpError> {
    if !source.bullets.iter().any(|b| b.name == "main" && b.params.is_empty()) {
        return Err(InterpError::new(
            "no 'main' bullet found — \
             define 'let main() -> result: Unit { ... }' as the entry point"
        ));
    }
    call_bullet("main", vec![], source)?;
    Ok(())
}

// ── Bullet execution ──────────────────────────────────────────────────────────

fn call_bullet(name: &str, args: Vec<Value>, source: &SourceFile) -> Result<Value, InterpError> {
    let bullet = source.bullets.iter()
        .find(|b| b.name == name)
        .ok_or_else(|| InterpError::new(format!("undefined bullet '{}'", name)))?;

    match &bullet.body {
        BulletBody::Pipes(pipes) => {
            let mut env: HashMap<String, Value> = HashMap::new();
            for (param, val) in bullet.params.iter().zip(args) {
                env.insert(param.name.clone(), val);
            }
            for pipe in pipes {
                eval_pipe(pipe, &mut env, source)
                    .map_err(|e| e.in_bullet(name))?;
            }
            let ret_name = bullet.output.as_ref().map(|o| o.name.as_str());
            Ok(ret_name.and_then(|n| env.get(n)).cloned().unwrap_or(Value::Unit))
        }
        BulletBody::Builtin(builtin_name) => {
            call_builtin(builtin_name, args)
                .map_err(|e| e.in_bullet(name))
        }
        BulletBody::Natives(_) => Err(InterpError::new(format!(
            "bullet '{}' contains native escape blocks and cannot be interpreted — \
             use 'bullarchy convert' to transpile instead",
            name
        ))),
    }
}

// ── Pipe evaluation ───────────────────────────────────────────────────────────

fn eval_pipe(
    pipe:   &Pipe,
    env:    &mut HashMap<String, Value>,
    source: &SourceFile,
) -> Result<(), InterpError> {
    // Evaluate input expressions — used for implicit arg passing.
    let input_vals = |env: &HashMap<String, Value>| -> Result<Vec<Value>, InterpError> {
        pipe.inputs.iter().map(|expr| eval_expr(expr, env, source)).collect()
    };

    let value = match &pipe.expr {
        // (a) : builtin::to_upper -> {result}  — implicit input passing
        Expr::Atom(Atom::BuiltinNoArgs(name)) => {
            call_builtin(name, input_vals(env)?)?
        }
        // (n) : square -> {result}  — bare ident naming a bullet, implicit input passing
        Expr::Atom(Atom::Ident(name))
            if source.bullets.iter().any(|b| &b.name == name) =>
        {
            call_bullet(name, input_vals(env)?, source)?
        }
        // Everything else: normal expression evaluation
        expr => eval_expr(expr, env, source)?,
    };

    if let Some(ref name) = pipe.binding {
        env.insert(name.clone(), value);
    }
    Ok(())
}

// ── Expression evaluation ─────────────────────────────────────────────────────

fn eval_expr(
    expr:   &Expr,
    env:    &HashMap<String, Value>,
    source: &SourceFile,
) -> Result<Value, InterpError> {
    match expr {
        Expr::Atom(a)      => eval_atom(a, env, source),
        Expr::BinOp(b)     => eval_binop(b, env, source),
        Expr::Tuple(exprs) => {
            let vals: Result<Vec<_>, _> = exprs.iter()
                .map(|e| eval_expr(e, env, source))
                .collect();
            Ok(Value::Tuple(vals?))
        }
    }
}

fn eval_atom(
    atom:   &Atom,
    env:    &HashMap<String, Value>,
    source: &SourceFile,
) -> Result<Value, InterpError> {
    match atom {
        Atom::Ident(name) => env.get(name)
            .cloned()
            .ok_or_else(|| InterpError::new(format!("undefined variable '{}'", name))),

        Atom::Integer(n)   => Ok(Value::Int(*n)),
        Atom::Float(f)     => Ok(Value::Float(*f)),
        Atom::StringLit(s) => Ok(Value::Str(unescape(s))),

        Atom::Interp(template) => {
            let mut result = unescape(template);
            for (key, val) in env {
                result = result.replace(&format!("{{{}}}", key), &val.to_string());
            }
            Ok(Value::Str(result))
        }

        Atom::Call { name, args } => {
            let vals: Result<Vec<_>, _> = args.iter().map(|a| match a {
                CallArg::Value(v) => env.get(v)
                    .cloned()
                    .ok_or_else(|| InterpError::new(format!("undefined variable '{}'", v))),
                CallArg::BulletRef(r) => Err(InterpError::new(format!(
                    "bullet references ('{}') are not supported in interpreted mode", r
                ))),
            }).collect();
            call_bullet(name, vals?, source)
        }

        Atom::Unary { op, rhs } => {
            let val = eval_atom(rhs, env, source)?;
            match op.as_str() {
                "-" => match val {
                    Value::Int(n)   => Ok(Value::Int(-n)),
                    Value::Float(f) => Ok(Value::Float(-f)),
                    _ => Err(InterpError::new("unary '-' requires a numeric value")),
                },
                "!" => match val {
                    Value::Bool(b) => Ok(Value::Bool(!b)),
                    _ => Err(InterpError::new("unary '!' requires a bool value")),
                },
                op => Err(InterpError::new(format!("unknown unary operator '{}'", op))),
            }
        }

        Atom::BuiltinExpr { name, args } => {
            let vals: Result<Vec<_>, _> = args.iter()
                .map(|e| eval_expr(e, env, source))
                .collect();
            call_builtin(name, vals?)
        }
        Atom::BuiltinNoArgs(name) => {
            // Outside a pipe context (e.g. nested in an expression),
            // a bare builtin::name with no resolvable inputs is an error.
            Err(InterpError::new(format!(
                "builtin '{}' used without arguments outside a pipe —                  use builtin::{}(arg) syntax or move it to a pipe", name, name
            )))
        }

        Atom::Index { base, idx } => {
            let base_val = env.get(base)
                .cloned()
                .ok_or_else(|| InterpError::new(format!("undefined variable '{}'", base)))?;
            let i = eval_expr(idx, env, source)?;
            match (base_val, i) {
                (Value::Str(s), Value::Int(i)) => s.chars()
                    .nth(i as usize)
                    .map(|c| Value::Str(c.to_string()))
                    .ok_or_else(|| InterpError::new(format!("index {} out of bounds", i))),
                (Value::Array(arr), Value::Int(i)) => arr.get(i as usize)
                    .cloned()
                    .ok_or_else(|| InterpError::new(format!("index {} out of bounds", i))),
                _ => Err(InterpError::new("index requires a String or Array and an i64")),
            }
        }

        Atom::Slice { base, from, to } => {
            let base_val = env.get(base)
                .cloned()
                .ok_or_else(|| InterpError::new(format!("undefined variable '{}'", base)))?;
            let from_val = eval_expr(from, env, source)?;
            let to_val   = eval_expr(to,   env, source)?;
            match (base_val, from_val, to_val) {
                (Value::Str(s), Value::Int(f), Value::Int(t)) => {
                    let result: String = s.chars()
                        .skip(f as usize)
                        .take((t - f) as usize)
                        .collect();
                    Ok(Value::Str(result))
                }
                (Value::Array(arr), Value::Int(f), Value::Int(t)) => {
                    let result = arr.into_iter()
                        .skip(f as usize)
                        .take((t - f) as usize)
                        .collect();
                    Ok(Value::Array(result))
                }
                _ => Err(InterpError::new("slice requires a String or Array and i64 bounds")),
            }
        }

        Atom::EnumVariant { variant, .. } => Ok(Value::Str(variant.clone())),

        Atom::FieldAccess { base, fields } => Err(InterpError::new(format!(
            "struct field access ('{}.{}') is not supported in interpreted mode",
            base, fields.join(".")
        ))),

        Atom::Closure { .. } => Err(InterpError::new(
            "closures are not supported in interpreted mode"
        )),
    }
}

// ── Binary operators ──────────────────────────────────────────────────────────

fn eval_binop(
    b:      &BinExpr,
    env:    &HashMap<String, Value>,
    source: &SourceFile,
) -> Result<Value, InterpError> {
    let lhs = eval_atom(&b.lhs, env, source)?;
    let rhs = eval_atom(&b.rhs, env, source)?;
    match b.op.as_str() {
        "+"  => add(lhs, rhs),
        "-"  => sub(lhs, rhs),
        "*"  => mul(lhs, rhs),
        "/"  => div(lhs, rhs),
        "%"  => rem(lhs, rhs),
        "==" => Ok(Value::Bool(eq(&lhs, &rhs))),
        "!=" => Ok(Value::Bool(!eq(&lhs, &rhs))),
        "<"  => cmp(lhs, rhs, |a, b| a < b,  |a, b| a < b),
        "<=" => cmp(lhs, rhs, |a, b| a <= b, |a, b| a <= b),
        ">"  => cmp(lhs, rhs, |a, b| a > b,  |a, b| a > b),
        ">=" => cmp(lhs, rhs, |a, b| a >= b, |a, b| a >= b),
        "&&" => match (lhs, rhs) {
            (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a && b)),
            _ => Err(InterpError::new("'&&' requires bool operands")),
        },
        "||" => match (lhs, rhs) {
            (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a || b)),
            _ => Err(InterpError::new("'||' requires bool operands")),
        },
        op => Err(InterpError::new(format!("unknown operator '{}'", op))),
    }
}

fn add(l: Value, r: Value) -> Result<Value, InterpError> {
    match (l, r) {
        (Value::Int(a),   Value::Int(b))   => Ok(Value::Int(a + b)),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (Value::Float(a), Value::Int(b))   => Ok(Value::Float(a + b as f64)),
        (Value::Int(a),   Value::Float(b)) => Ok(Value::Float(a as f64 + b)),
        (Value::Str(a),   Value::Str(b))   => Ok(Value::Str(a + &b)),
        _ => Err(InterpError::new("'+' requires numeric or string operands")),
    }
}

fn sub(l: Value, r: Value) -> Result<Value, InterpError> {
    match (l, r) {
        (Value::Int(a),   Value::Int(b))   => Ok(Value::Int(a - b)),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
        (Value::Float(a), Value::Int(b))   => Ok(Value::Float(a - b as f64)),
        (Value::Int(a),   Value::Float(b)) => Ok(Value::Float(a as f64 - b)),
        _ => Err(InterpError::new("'-' requires numeric operands")),
    }
}

fn mul(l: Value, r: Value) -> Result<Value, InterpError> {
    match (l, r) {
        (Value::Int(a),   Value::Int(b))   => Ok(Value::Int(a * b)),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
        (Value::Float(a), Value::Int(b))   => Ok(Value::Float(a * b as f64)),
        (Value::Int(a),   Value::Float(b)) => Ok(Value::Float(a as f64 * b)),
        _ => Err(InterpError::new("'*' requires numeric operands")),
    }
}

fn div(l: Value, r: Value) -> Result<Value, InterpError> {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => {
            if b == 0 { return Err(InterpError::new("division by zero")); }
            Ok(Value::Int(a / b))
        }
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
        (Value::Float(a), Value::Int(b))   => Ok(Value::Float(a / b as f64)),
        (Value::Int(a),   Value::Float(b)) => Ok(Value::Float(a as f64 / b)),
        _ => Err(InterpError::new("'/' requires numeric operands")),
    }
}

fn rem(l: Value, r: Value) -> Result<Value, InterpError> {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => {
            if b == 0 { return Err(InterpError::new("modulo by zero")); }
            Ok(Value::Int(a % b))
        }
        _ => Err(InterpError::new("'%' requires integer operands")),
    }
}

fn eq(l: &Value, r: &Value) -> bool {
    match (l, r) {
        (Value::Int(a),   Value::Int(b))   => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::Bool(a),  Value::Bool(b))  => a == b,
        (Value::Str(a),   Value::Str(b))   => a == b,
        _                                  => false,
    }
}

fn cmp<FI, FF>(l: Value, r: Value, fi: FI, ff: FF) -> Result<Value, InterpError>
where
    FI: Fn(i64, i64) -> bool,
    FF: Fn(f64, f64) -> bool,
{
    match (l, r) {
        (Value::Int(a),   Value::Int(b))   => Ok(Value::Bool(fi(a, b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(ff(a, b))),
        _ => Err(InterpError::new("comparison requires numeric operands of the same type")),
    }
}

// ── Builtin dispatch ──────────────────────────────────────────────────────────

fn call_builtin(name: &str, args: Vec<Value>) -> Result<Value, InterpError> {
    match name {
        // ── Math ──────────────────────────────────────────────────────────────
        "abs"  => one_num(&args, "abs",  |i| Value::Int(i.abs()),   |f| Value::Float(f.abs())),
        "sqrt" => one_f64(&args, "sqrt", |f| Value::Float(f.sqrt())),
        "exp"  => one_f64(&args, "exp",  |f| Value::Float(f.exp())),
        "log"  => {
            n_args("log", &args, 2)?;
            match (&args[0], &args[1]) {
                (Value::Float(x), Value::Float(base)) => Ok(Value::Float(x.log(*base))),
                (Value::Int(x),   Value::Float(base)) => Ok(Value::Float((*x as f64).log(*base))),
                (Value::Float(x), Value::Int(base))   => Ok(Value::Float(x.log(*base as f64))),
                (Value::Int(x),   Value::Int(base))   => Ok(Value::Float((*x as f64).log(*base as f64))),
                _ => Err(InterpError::new("log requires two numeric arguments")),
            }
        }
        "pow"  => two_i64(&args, "pow",  |a, b| Value::Int(a.pow(b as u32))),
        "powf" => two_f64(&args, "powf", |a, b| Value::Float(a.powf(b))),
        "min"  => two_num(&args, "min",  |a, b| Value::Int(a.min(b)),   |a, b| Value::Float(a.min(b))),
        "max"  => two_num(&args, "max",  |a, b| Value::Int(a.max(b)),   |a, b| Value::Float(a.max(b))),
        "clamp" => {
            n_args("clamp", &args, 3)?;
            match (&args[0], &args[1], &args[2]) {
                (Value::Int(v),   Value::Int(lo),   Value::Int(hi))   => Ok(Value::Int((*v).clamp(*lo, *hi))),
                (Value::Float(v), Value::Float(lo), Value::Float(hi)) => Ok(Value::Float(v.clamp(*lo, *hi))),
                _ => Err(InterpError::new("clamp requires three numeric arguments of the same type")),
            }
        }

        // ── Conditions ────────────────────────────────────────────────────────
        "tern" => {
            n_args("tern", &args, 3)?;
            match &args[0] {
                Value::Bool(true)  => Ok(args[1].clone()),
                Value::Bool(false) => Ok(args[2].clone()),
                _ => Err(InterpError::new("tern: first argument must be a Bool")),
            }
        }

        // ── String ────────────────────────────────────────────────────────────
        "to_upper"  => one_str(&args, "to_upper",  |s| Value::Str(s.to_uppercase())),
        "to_lower"  => one_str(&args, "to_lower",  |s| Value::Str(s.to_lowercase())),
        "trim"      => one_str(&args, "trim",      |s| Value::Str(s.trim().to_string())),
        "len"       => {
            n_args("len", &args, 1)?;
            match &args[0] {
                Value::Str(s)   => Ok(Value::Int(s.chars().count() as i64)),
                Value::Array(a) => Ok(Value::Int(a.len() as i64)),
                _ => Err(InterpError::new("len requires a String or Array argument")),
            }
        }
        "to_string" => {
            n_args("to_string", &args, 1)?;
            Ok(Value::Str(args[0].to_string()))
        }
        "parse_i64" => one_str(&args, "parse_i64", |s| {
            Value::Int(s.trim().parse::<i64>().unwrap_or(0))
        }),
        "starts_with" => {
            n_args("starts_with", &args, 2)?;
            match (&args[0], &args[1]) {
                (Value::Str(s), Value::Str(p)) => Ok(Value::Bool(s.starts_with(p.as_str()))),
                _ => Err(InterpError::new("starts_with requires two String arguments")),
            }
        }
        "ends_with" => {
            n_args("ends_with", &args, 2)?;
            match (&args[0], &args[1]) {
                (Value::Str(s), Value::Str(p)) => Ok(Value::Bool(s.ends_with(p.as_str()))),
                _ => Err(InterpError::new("ends_with requires two String arguments")),
            }
        }
        "replace_str" => {
            n_args("replace_str", &args, 3)?;
            match (&args[0], &args[1], &args[2]) {
                (Value::Str(s), Value::Str(from), Value::Str(to)) => {
                    Ok(Value::Str(s.replace(from.as_str(), to.as_str())))
                }
                _ => Err(InterpError::new("replace_str requires three String arguments")),
            }
        }

        // ── Algorithms ────────────────────────────────────────────────────────
        "swap" => {
            n_args("swap", &args, 2)?;
            Ok(Value::Tuple(vec![args[1].clone(), args[0].clone()]))
        }

        "insertion_sort" => {
            n_args("insertion_sort", &args, 1)?;
            let mut arr = as_i64_array(&args[0], "insertion_sort")?;
            let n = arr.len();
            for i in 1..n {
                let key = arr[i];
                let mut j = i;
                while j > 0 && arr[j - 1] > key {
                    arr[j] = arr[j - 1];
                    j -= 1;
                }
                arr[j] = key;
            }
            Ok(Value::Array(arr.into_iter().map(Value::Int).collect()))
        }

        "quick_sort" => {
            n_args("quick_sort", &args, 1)?;
            let mut arr = as_i64_array(&args[0], "quick_sort")?;
            quicksort(&mut arr);
            Ok(Value::Array(arr.into_iter().map(Value::Int).collect()))
        }

        "merge_sort" => {
            n_args("merge_sort", &args, 1)?;
            let arr = as_i64_array(&args[0], "merge_sort")?;
            let sorted = mergesort(arr);
            Ok(Value::Array(sorted.into_iter().map(Value::Int).collect()))
        }

        "radix_sort" => {
            n_args("radix_sort", &args, 1)?;
            let mut arr = as_i64_array(&args[0], "radix_sort")?;
            radixsort(&mut arr);
            Ok(Value::Array(arr.into_iter().map(Value::Int).collect()))
        }

        // ── I/O ───────────────────────────────────────────────────────────────
        "out" => {
            n_args("out", &args, 2)?;
            match (&args[0], &args[1]) {
                (Value::Int(fd), content) => {
                    let text = content.to_string();
                    match fd {
                        1 => { print!("{}", text); io::stdout().flush().ok(); }
                        2 => { eprint!("{}", text); io::stderr().flush().ok(); }
                        fd => {
                            use std::io::Write as _;
                            let bytes = text.as_bytes();
                            with_fd_table(|t| {
                                t.files.get_mut(fd)
                                    .ok_or_else(|| InterpError::new(format!("out: fd {} is not open", fd)))
                                    .and_then(|f| {
                                        f.write_all(bytes)
                                            .map_err(|e| InterpError::new(format!("out: {}", e)))
                                    })
                            })?;
                        }
                    }
                    Ok(Value::Int(text.len() as i64))
                }
                _ => Err(InterpError::new("out requires (fd: i32, content: String)")),
            }
        }

        "in" => {
            n_args("in", &args, 1)?;
            match &args[0] {
                Value::Int(fd) => match fd {
                    0 | 1 => {
                        let mut line = String::new();
                        io::stdin().lock().read_line(&mut line).ok();
                        Ok(Value::Str(line.trim_end_matches('\n').to_string()))
                    }
                    fd => {
                        use std::io::Read;
                        let fd = *fd;
                        with_fd_table(|t| {
                            let file = t.files.get_mut(&fd)
                                .ok_or_else(|| InterpError::new(format!("in: fd {} is not open", fd)))?;
                            let mut line = String::new();
                            let mut byte = [0u8; 1];
                            loop {
                                match file.read(&mut byte) {
                                    Ok(0) => break,
                                    Ok(_) => {
                                        let ch = byte[0] as char;
                                        if ch == '\n' { break; }
                                        line.push(ch);
                                    }
                                    Err(e) => return Err(InterpError::new(format!("in: {}", e))),
                                }
                            }
                            Ok(Value::Str(line))
                        })
                    }
                },
                _ => Err(InterpError::new("in requires (fd: i32)")),
            }
        }

        "open" => {
            n_args("open", &args, 2)?;
            match (&args[0], &args[1]) {
                (Value::Str(path), Value::Str(mode)) => {
                    let path = path.clone();
                    let mode = mode.clone();
                    with_fd_table(|t| t.open(&path, &mode).map(Value::Int))
                }
                _ => Err(InterpError::new("open requires (path: String, mode: String)")),
            }
        }

        "close" => {
            n_args("close", &args, 1)?;
            match &args[0] {
                Value::Int(fd) => {
                    let fd = *fd;
                    with_fd_table(|t| t.close(fd))?;
                    Ok(Value::Unit)
                }
                _ => Err(InterpError::new("close requires (fd: i64)")),
            }
        }

        "time" => {
            use std::time::{SystemTime, UNIX_EPOCH};
            let secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            Ok(Value::Int(secs))
        }

        // ── System ────────────────────────────────────────────────────────────
        "args" => {
            let collected: Vec<Value> = std::env::args()
                .skip(1)
                .map(Value::Str)
                .collect();
            Ok(Value::Array(collected))
        }

        "exit" => {
            n_args("exit", &args, 1)?;
            match &args[0] {
                Value::Int(code) => std::process::exit(*code as i32),
                _ => Err(InterpError::new("exit requires an i64 exit code")),
            }
        }

        "env" => {
            n_args("env", &args, 1)?;
            match &args[0] {
                Value::Str(key) => Ok(Value::Str(
                    std::env::var(key).unwrap_or_default()
                )),
                _ => Err(InterpError::new("env requires a String key")),
            }
        }

        "sleep" => {
            n_args("sleep", &args, 1)?;
            match &args[0] {
                Value::Int(ms) => {
                    std::thread::sleep(std::time::Duration::from_millis(*ms as u64));
                    Ok(Value::Unit)
                }
                _ => Err(InterpError::new("sleep requires an i64 millisecond count")),
            }
        }

        _ => Err(InterpError::new(format!("unknown builtin '{}'", name))),
    }
}

// ── Sorting implementations ───────────────────────────────────────────────────

fn as_i64_array(v: &Value, name: &str) -> Result<Vec<i64>, InterpError> {
    match v {
        Value::Array(arr) => arr.iter().map(|item| match item {
            Value::Int(n) => Ok(*n),
            _ => Err(InterpError::new(format!("{}: array must contain i64 values", name))),
        }).collect(),
        _ => Err(InterpError::new(format!("{} requires an Array argument", name))),
    }
}

fn quicksort(arr: &mut [i64]) {
    if arr.len() <= 1 { return; }
    let pivot = arr[arr.len() / 2];
    let mut lo = 0;
    let mut hi = arr.len() - 1;
    while lo <= hi {
        while arr[lo] < pivot { lo += 1; }
        while arr[hi] > pivot { if hi == 0 { break; } hi -= 1; }
        if lo <= hi {
            arr.swap(lo, hi);
            lo += 1;
            if hi > 0 { hi -= 1; }
        }
    }
    if hi + 1 > 1 { quicksort(&mut arr[..hi + 1]); }
    if lo < arr.len() { quicksort(&mut arr[lo..]); }
}

fn mergesort(arr: Vec<i64>) -> Vec<i64> {
    if arr.len() <= 1 { return arr; }
    let mid = arr.len() / 2;
    let left  = mergesort(arr[..mid].to_vec());
    let right = mergesort(arr[mid..].to_vec());
    merge(left, right)
}

fn merge(left: Vec<i64>, right: Vec<i64>) -> Vec<i64> {
    let mut result = Vec::with_capacity(left.len() + right.len());
    let (mut i, mut j) = (0, 0);
    while i < left.len() && j < right.len() {
        if left[i] <= right[j] { result.push(left[i]);  i += 1; }
        else                    { result.push(right[j]); j += 1; }
    }
    result.extend_from_slice(&left[i..]);
    result.extend_from_slice(&right[j..]);
    result
}

fn radixsort(arr: &mut Vec<i64>) {
    if arr.is_empty() { return; }
    let max = *arr.iter().max().unwrap();
    let mut exp = 1i64;
    while max / exp > 0 {
        let mut output = vec![0i64; arr.len()];
        let mut count  = vec![0usize; 10];
        for &n in arr.iter() { count[((n / exp) % 10) as usize] += 1; }
        for i in 1..10 { count[i] += count[i - 1]; }
        for &n in arr.iter().rev() {
            let digit = ((n / exp) % 10) as usize;
            count[digit] -= 1;
            output[count[digit]] = n;
        }
        arr.clone_from(&output);
        if exp > i64::MAX / 10 { break; }
        exp *= 10;
    }
}

// ── Argument helpers ──────────────────────────────────────────────────────────

fn n_args(name: &str, args: &[Value], n: usize) -> Result<(), InterpError> {
    if args.len() == n { Ok(()) } else {
        Err(InterpError::new(format!(
            "{} expects {} argument(s), got {}", name, n, args.len()
        )))
    }
}

fn one_num<FI, FF>(args: &[Value], name: &str, fi: FI, ff: FF) -> Result<Value, InterpError>
where FI: Fn(i64) -> Value, FF: Fn(f64) -> Value {
    n_args(name, args, 1)?;
    match &args[0] {
        Value::Int(n)   => Ok(fi(*n)),
        Value::Float(f) => Ok(ff(*f)),
        _ => Err(InterpError::new(format!("{} requires a numeric argument", name))),
    }
}

fn one_f64<F>(args: &[Value], name: &str, f: F) -> Result<Value, InterpError>
where F: Fn(f64) -> Value {
    n_args(name, args, 1)?;
    match &args[0] {
        Value::Float(n) => Ok(f(*n)),
        Value::Int(n)   => Ok(f(*n as f64)),
        _ => Err(InterpError::new(format!("{} requires a numeric argument", name))),
    }
}

fn one_str<F>(args: &[Value], name: &str, f: F) -> Result<Value, InterpError>
where F: Fn(&str) -> Value {
    n_args(name, args, 1)?;
    match &args[0] {
        Value::Str(s) => Ok(f(s)),
        _ => Err(InterpError::new(format!("{} requires a String argument", name))),
    }
}

fn two_i64<F>(args: &[Value], name: &str, f: F) -> Result<Value, InterpError>
where F: Fn(i64, i64) -> Value {
    n_args(name, args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => Ok(f(*a, *b)),
        _ => Err(InterpError::new(format!("{} requires two i64 arguments", name))),
    }
}

fn two_f64<F>(args: &[Value], name: &str, f: F) -> Result<Value, InterpError>
where F: Fn(f64, f64) -> Value {
    n_args(name, args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Float(a), Value::Float(b)) => Ok(f(*a, *b)),
        (Value::Int(a),   Value::Int(b))   => Ok(f(*a as f64, *b as f64)),
        _ => Err(InterpError::new(format!("{} requires two numeric arguments", name))),
    }
}

fn two_num<FI, FF>(args: &[Value], name: &str, fi: FI, ff: FF) -> Result<Value, InterpError>
where FI: Fn(i64, i64) -> Value, FF: Fn(f64, f64) -> Value {
    n_args(name, args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Int(a),   Value::Int(b))   => Ok(fi(*a, *b)),
        (Value::Float(a), Value::Float(b)) => Ok(ff(*a, *b)),
        _ => Err(InterpError::new(format!(
            "{} requires two numeric arguments of the same type", name
        ))),
    }
}
