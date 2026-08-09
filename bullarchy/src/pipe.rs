//! What a bullet's right-hand side actually is.
//!
//! A bullet is `(inputs) : expr -> {binding};`, and the inputs mean different
//! things depending on what `expr` is. Sometimes they are the arguments to it;
//! sometimes `expr` already names its own arguments and they are just the
//! values it reads. Every backend decided this for itself, and all six decided
//! it the same wrong way:
//!
//! ```text
//! match &pipe.expr {
//!     Expr::Atom(Atom::Call { .. }) => base,
//!     _ => format!("{}({})", base, inputs_str),
//! }
//! ```
//!
//! Anything that was not literally a call had the inputs appended to it, so
//! `(a, b) : a + b -> {sum};` emitted `a + b(a, b)` — six times over, in six
//! languages. The type checker had the matching gap from the other side: it
//! reached `Atom::BuiltinNoArgs` through `infer_atom`, which never sees
//! `pipe.inputs` at all, and hit an `unreachable!()`.
//!
//! Both need the same four-way answer, so it is made once, here:
//!
//! | Bullet                                | Right-hand side          |
//! |---------------------------------------|--------------------------|
//! | `(a, b) : a + b -> {sum};`            | `a + b`                  |
//! | `(a, b) : add -> {sum};`              | `add(a, b)`              |
//! | `(s) : builtin::to_upper -> {r};`     | the builtin, inputs as args |
//! | `(x) : some_fn(x, 2) -> {r};`         | `some_fn(x, 2)`          |
//!
//! Each backend then emits `binding = <rhs>;` and nothing else.

use bullang::ast::{Atom, Expr, Pipe};

/// A bullet's right-hand side, classified.
pub enum PipeRhs<'a> {
    /// Complete on its own — a binary operation, a literal, a field access, or
    /// a call that already spells out its arguments. The pipe's inputs are
    /// values it reads, not arguments to append.
    Expr(&'a Expr),
    /// A bare function name. The pipe's inputs are its arguments.
    Call { name: &'a str, args: &'a [Expr] },
    /// A bare builtin name. The pipe's inputs are its arguments.
    Builtin { name: &'a str, args: &'a [Expr] },
}

/// Classify `pipe`'s right-hand side.
pub fn classify(pipe: &Pipe) -> PipeRhs<'_> {
    match &pipe.expr {
        // `builtin::to_upper` with no argument list: the inputs are the
        // arguments. This is the form the whole pipe syntax exists for.
        Expr::Atom(Atom::BuiltinNoArgs(name)) => PipeRhs::Builtin {
            name: name.as_str(),
            args: &pipe.inputs,
        },

        // A bare name with inputs is a call: `(a, b) : add` is `add(a, b)`.
        // With no inputs it is just the value of that name — `() : x` is `x`,
        // not `x()`, because a call with no arguments would be written that
        // way.
        Expr::Atom(Atom::Ident(name)) if !pipe.inputs.is_empty() => PipeRhs::Call {
            name: name.as_str(),
            args: &pipe.inputs,
        },

        // Everything else stands on its own. `some_fn(x, 2)` already has its
        // arguments; `a + b` is an operation, not a callee; a literal is a
        // value. None of them take the inputs appended.
        other => PipeRhs::Expr(other),
    }
}

/// A builtin's arguments, as the `Param` list its emitter expects.
///
/// A builtin emitter works on the *text* of its arguments — `to_upper` turns
/// `s` into `s.to_uppercase()` — so each parameter is named by whatever the
/// backend emits for that expression. The old code only did this for inputs
/// that were plain identifiers and named everything else `__pipe_arg_0`,
/// which the builtin then emitted verbatim into code where no such name was
/// ever declared.
///
/// `ty_of` supplies the argument's Bullang type. Only `swap` reads it, to
/// name the `Tuple[T, T]` struct C and Go return it in; every other builtin
/// works on the text alone, so a backend without type inference can pass
/// [`no_types`].
///
/// Backends whose arguments need hoisting into temporaries first (C, C++, Go,
/// Java, Python) build the list themselves — where to put the temporary is a
/// backend question. Only Rust, which needs no temporaries, uses this.
pub fn builtin_params(
    args:  &[Expr],
    emit:  &dyn Fn(&Expr) -> String,
    ty_of: &dyn Fn(&Expr) -> bullang::ast::BuType,
) -> Vec<bullang::ast::Param> {
    args.iter()
        .map(|arg| bullang::ast::Param {
            name: emit(arg),
            ty:   ty_of(arg),
        })
        .collect()
}

/// `ty_of` for backends that do no type inference.
pub fn no_types(_: &Expr) -> bullang::ast::BuType {
    bullang::ast::BuType::Unknown
}

/// An inline `builtin::name(args)` used as an expression.
///
/// This used to be a separate hand-written table in each backend that handled
/// exactly three names — `assert`, `assert_eq` and `assert_ne` — none of which
/// are in the catalogue or emitted anywhere else. Every *real* builtin fell
/// through to a default arm that emitted a comment, so `builtin::to_upper(s)`
/// written inline produced `/* builtin::to_upper not supported as expression */`
/// where the value should have been. C++ had no arm at all.
///
/// There is nothing special about inline position: the same builtin, the same
/// emitter, the same arguments. The only difference from a bullet is that the
/// arguments are written at the call site rather than taken from the pipe.
pub fn inline_builtin(
    name:    &str,
    args:    &[Expr],
    backend: &bullang::ast::Backend,
    emit:    &dyn Fn(&Expr) -> String,
) -> Result<String, String> {
    // A string-returning builtin in C writes into a buffer the caller
    // declares, which inline position has nowhere to put. A bullet does.
    if matches!(backend, bullang::ast::Backend::C)
        && crate::stdlib::returns_string_in_c(name)
    {
        return Err(format!(
            "'builtin::{name}' returns a String, which the C backend cannot \
             produce in expression position — it needs a destination to write \
             into. Give it its own bullet: `(...) : builtin::{name} -> {{r}};`"
        ));
    }
    let params = builtin_params(args, emit, &no_types);
    crate::stdlib::emit_builtin(name, &params, backend)
}
