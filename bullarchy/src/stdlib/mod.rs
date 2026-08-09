//! The core standard library: one module per builtin.
//!
//! Bullang's core is small on purpose. Anything beyond it — maths, sorting,
//! networking — lives in a package installed with `bullarchy add` and declared
//! with `#use:`. This module owns emission for the core set only.
//!
//! Each builtin module exposes three things:
//!
//!   - `META`         the catalogue entry, re-exported from the bullang crate
//!   - `emit`         the code for one backend
//!   - `requirements` the imports, includes and helper functions that code needs
//!
//! `requirements` lives next to `emit` deliberately. It used to be a central
//! match table in this file, which meant adding a builtin required editing two
//! places and nothing caught the omission — `builtin::time` shipped needing
//! `<time.h>` with no entry for it, and the generated C did not compile. A
//! builtin now carries everything it needs in one file.

use bullang::ast::{Backend, Param};

mod args;
mod c_str;
mod cpp_str;
mod close;
mod ends_with;
mod env;
mod exit;
mod fd_in;
mod fd_out;
mod i64_to_str;
mod len;
mod max;
mod min;
mod open;
mod replace_str;
mod run;
mod sleep;
mod starts_with;
mod str_to_i64;
mod swap;
mod tern;
mod time;
mod to_lower;
mod to_upper;
mod trim;

// ── What a builtin needs at file scope ────────────────────────────────────

/// Everything a builtin's emitted code needs declared above it.
///
/// `imports` are `use` / `#include` / `import` lines; `helpers` are whole
/// function definitions dropped in once per file. Both are deduplicated and
/// hoisted by the backend, so a builtin used ten times contributes them once.
#[derive(Default)]
pub struct Requirements {
    /// Needed by the emitted call itself — `strings.ToUpper(s)` needs
    /// `strings` wherever it appears.
    pub imports: &'static [&'static str],
    pub helpers: &'static [&'static str],
    /// Needed by the *helpers*, not by the call. These are separate because
    /// Go declares helpers once per package, in their own file, and treats an
    /// unused import as an error — so an import a helper needs must travel
    /// with the helper, not with every file that calls it.
    pub helper_imports: &'static [&'static str],
}

impl Requirements {
    pub const NONE: Requirements = Requirements { imports: &[], helpers: &[], helper_imports: &[] };

    pub const fn imports(imports: &'static [&'static str]) -> Self {
        Requirements { imports, helpers: &[], helper_imports: &[] }
    }

    pub const fn new(imports: &'static [&'static str], helpers: &'static [&'static str]) -> Self {
        Requirements { imports, helpers, helper_imports: &[] }
    }

    /// Helpers whose imports belong with them rather than at the call site.
    pub const fn helper(
        helpers:        &'static [&'static str],
        helper_imports: &'static [&'static str],
    ) -> Self {
        Requirements { imports: &[], helpers, helper_imports }
    }
}

/// Everything `name` needs at file scope for `backend`.
pub fn requirements(name: &str, backend: &Backend) -> Requirements {
    match name {
        "min"         => min::requirements(backend),
        "max"         => max::requirements(backend),
        "tern"        => tern::requirements(backend),
        "to_upper"    => to_upper::requirements(backend),
        "to_lower"    => to_lower::requirements(backend),
        "trim"        => trim::requirements(backend),
        "starts_with" => starts_with::requirements(backend),
        "ends_with"   => ends_with::requirements(backend),
        "replace_str" => replace_str::requirements(backend),
        "i64_to_str"  => i64_to_str::requirements(backend),
        "str_to_i64"  => str_to_i64::requirements(backend),
        "len"         => len::requirements(backend),
        "swap"        => swap::requirements(backend),
        "in"          => fd_in::requirements(backend),
        "out"         => fd_out::requirements(backend),
        "open"        => open::requirements(backend),
        "close"       => close::requirements(backend),
        "time"        => time::requirements(backend),
        "argc"        => args::requirements(backend),
        "args"        => args::requirements(backend),
        "exit"        => exit::requirements(backend),
        "env"         => env::requirements(backend),
        "sleep"       => sleep::requirements(backend),
        "run"         => run::requirements(backend),
        _ => Requirements::NONE,
    }
}

// ── Known names ───────────────────────────────────────────────────────────

/// True if `name` is a core builtin, or one supplied by an enabled package.
pub fn is_known_builtin(name: &str) -> bool {
    if bullang::stdlib::is_core_builtin(name) {
        return true;
    }
    #[cfg(feature = "mathlib")]
    if bull_mathlib::is_known_builtin(name) {
        return true;
    }
    #[cfg(feature = "netlib")]
    if bull_netlib::is_known_builtin(name) {
        return true;
    }
    false
}

// ── Dispatch ──────────────────────────────────────────────────────────────

pub fn emit_builtin(name: &str, params: &[Param], backend: &Backend) -> Result<String, String> {
    match name {
        "min"         => return min::emit(params, backend),
        "max"         => return max::emit(params, backend),
        "tern"        => return tern::emit(params, backend),
        "to_upper"    => return to_upper::emit(params, backend),
        "to_lower"    => return to_lower::emit(params, backend),
        "trim"        => return trim::emit(params, backend),
        "starts_with" => return starts_with::emit(params, backend),
        "ends_with"   => return ends_with::emit(params, backend),
        "replace_str" => return replace_str::emit(params, backend),
        "i64_to_str"  => return i64_to_str::emit(params, backend),
        "str_to_i64"  => return str_to_i64::emit(params, backend),
        "len"         => return len::emit(params, backend),
        "swap"        => return swap::emit(params, backend),
        "in"          => return fd_in::emit(params, backend),
        "out"         => return fd_out::emit(params, backend),
        "open"        => return open::emit(params, backend),
        "close"       => return close::emit(params, backend),
        "time"        => return time::emit(params, backend),
        "argc"        => return args::emit_argc(params, backend),
        "args"        => return args::emit(params, backend),
        "exit"        => return exit::emit(params, backend),
        "env"         => return env::emit(params, backend),
        "sleep"       => return sleep::emit(params, backend),
        "run"         => return run::emit(params, backend),
        _ => {}
    }

    #[cfg(feature = "mathlib")]
    if bull_mathlib::is_known_builtin(name) {
        return bull_mathlib::emit(name, params, backend);
    }
    #[cfg(feature = "netlib")]
    if bull_netlib::is_known_builtin(name) {
        return bull_netlib::emit(name, params, backend);
    }

    Err(format!(
        "'builtin::{}' is not a known builtin. Run `bullang stdlib` to see the core set. \
         If it belongs to a package, install it with `bullarchy add <name>` and declare \
         it with `#use:`.",
        name
    ))
}

// ── C: builtins that return a string ──────────────────────────────────────
//
// C is the one backend with no owning string type, so a builtin that returns
// a String cannot return an expression: it has nowhere to put the bytes. The
// old code returned pointers into `malloc` (never freed) or wrote through the
// argument pointer (undefined behaviour on a string literal, which is what
// `("hello") : builtin::to_upper -> {r};` passes).
//
// Instead the caller supplies the destination, `ft_strcpy`-style. The C
// backend declares it immediately before the bullet, sized from values in
// scope at that point, and no allocation happens at all:
//
//     char t[ft_strlen(s) + 1];
//     ft_trim(t, s);
//     char r[ft_strlen(t) + 1];
//     ft_to_upper(r, t);
//
// `emit` is still the entry point for every other backend and for every
// non-string builtin in C. A builtin implements `emit_c_dest` only if it
// returns a String, and `returns_string_in_c` tells the backend which path
// to take before it commits to emitting `binding = <rhs>;`.

/// A string-returning builtin's C form: how big the destination must be, and
/// the call that fills it.
pub struct CDest {
    /// Expression for the array size, e.g. `ft_strlen(s) + 1`. Valid only at
    /// the point of declaration, which is why the backend must emit it there
    /// and not hoist it.
    pub size: String,
    /// The call itself, e.g. `ft_to_upper(r, s)`. No trailing semicolon.
    pub call: String,
}

/// Where `helper` falls in the backend's declaration order, if it has
/// dependants. Only C and C++ require declaration before use.
pub fn helper_rank(helper: &str, backend: &Backend) -> Option<usize> {
    let order: &[&str] = match backend {
        Backend::C   => c_str::ORDER,
        Backend::Cpp => cpp_str::ORDER,
        _            => return None,
    };
    order.iter().position(|h| *h == helper)
}

/// True if `name` returns a String when emitted for C, and so must go through
/// [`emit_c_dest`] rather than [`emit_builtin`].
pub fn returns_string_in_c(name: &str) -> bool {
    matches!(
        name,
        "to_upper" | "to_lower" | "trim" | "replace_str" | "i64_to_str" | "in"
    )
}

/// The C form of a string-returning builtin, writing into `dest`.
///
/// Returns `None` for any builtin that is not string-returning in C — those
/// go through [`emit_builtin`] like every other backend.
pub fn emit_c_dest(name: &str, dest: &str, params: &[Param]) -> Result<Option<CDest>, String> {
    Ok(Some(match name {
        "to_upper"    => to_upper::emit_c_dest(dest, params)?,
        "to_lower"    => to_lower::emit_c_dest(dest, params)?,
        "trim"        => trim::emit_c_dest(dest, params)?,
        "replace_str" => replace_str::emit_c_dest(dest, params)?,
        "i64_to_str"  => i64_to_str::emit_c_dest(dest, params)?,
        "in"          => fd_in::emit_c_dest(dest, params)?,
        _ => return Ok(None),
    }))
}

// ── Shared helpers, available to every builtin module ─────────────────────

/// Parameter names.
pub(crate) fn p(params: &[Param]) -> Vec<&str> {
    params.iter().map(|p| p.name.as_str()).collect()
}

/// Assert `params` has exactly `n` entries; return their names.
pub(crate) fn need<'a>(name: &str, params: &'a [Param], n: usize) -> Result<Vec<&'a str>, String> {
    let v = p(params);
    if v.len() != n {
        return Err(format!(
            "'builtin::{}' takes {} argument(s) but was given {}", name, n, v.len()
        ));
    }
    Ok(v)
}

/// Rejection message for a backend that has no implementation of a builtin.
pub(crate) fn unsupported(name: &str, backend: &Backend) -> String {
    format!(
        "'builtin::{}' is not available for the '{}' backend",
        name, backend.escape_keyword()
    )
}
