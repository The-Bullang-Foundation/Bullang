//! Standard library — universal builtin functions.
//!
//! Only builtins that can be implemented in ALL five backends are included.
//! What remains is a clean set of 18 primitives: math, string, and algorithm
//! builtins that are fully inlined — no call to an existing language primitive.
//!
//! Syntax in source files:  builtin::abs   builtin::to_upper   etc.
//!
//! Each builtin lives in its own submodule and exposes:
//!   - `META : (&str, &str, &str)`  — (name, signature, description)
//!   - `emit(params, backend)`      — code-generation entry point
//!
//! Backends: Rust, Python, C, C++, Go

use crate::ast::{Backend, Param};

mod abs;
mod args;
mod clamp;
mod close;
mod ends_with;
mod env;
mod exit;
mod fd_in;
mod fd_out;
mod insertion_sort;
mod len;
mod max;
mod merge_sort;
mod min;
mod open;
mod parse_i64;
mod pow;
mod powf;
mod quick_sort;
mod radix_sort;
mod replace_str;
mod sleep;
mod sqrt;
mod starts_with;
mod swap;
mod time;
mod to_lower;
mod to_string;
mod to_upper;
mod trim;

// ── Universal builtin set ─────────────────────────────────────────────────────

/// The 31 universal builtins — available in every backend.
pub const BUILTINS: &[(&str, &str, &str)] = &[
    // math
    abs::META,
    pow::META,
    powf::META,
    sqrt::META,
    clamp::META,
    min::META,
    max::META,
    // string
    to_upper::META,
    to_lower::META,
    trim::META,
    starts_with::META,
    ends_with::META,
    replace_str::META,
    to_string::META,
    parse_i64::META,
    len::META,
    // algorithms
    swap::META,
    insertion_sort::META,
    quick_sort::META,
    merge_sort::META,
    radix_sort::META,
    // io
    fd_in::META,
    fd_out::META,
    open::META,
    close::META,
    time::META,
    // system
    args::META,
    exit::META,
    env::META,
    sleep::META,
];

/// Returns true if the name is a known universal builtin.
pub fn is_known_builtin(name: &str) -> bool {
    BUILTINS.iter().any(|(n, _, _)| *n == name)
}

/// Return the builtin catalogue.
pub fn list_builtins() -> Vec<(&'static str, &'static str, &'static str)> {
    BUILTINS.to_vec()
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

pub fn emit_builtin(name: &str, params: &[Param], backend: &Backend) -> Result<String, String> {
    if !is_known_builtin(name) {
        return Err(format!(
            "'builtin::{}' is not a known builtin. \
             Run `bullang stdlib --list` to see available builtins.",
            name
        ));
    }
    match name {
        "abs"            => abs::emit(params, backend),
        "pow"            => pow::emit(params, backend),
        "powf"           => powf::emit(params, backend),
        "sqrt"           => sqrt::emit(params, backend),
        "clamp"          => clamp::emit(params, backend),
        "min"            => min::emit(params, backend),
        "max"            => max::emit(params, backend),
        "to_upper"       => to_upper::emit(params, backend),
        "to_lower"       => to_lower::emit(params, backend),
        "trim"           => trim::emit(params, backend),
        "starts_with"    => starts_with::emit(params, backend),
        "ends_with"      => ends_with::emit(params, backend),
        "replace_str"    => replace_str::emit(params, backend),
        "to_string"      => to_string::emit(params, backend),
        "parse_i64"      => parse_i64::emit(params, backend),
        "len"            => len::emit(params, backend),
        "swap"           => swap::emit(params, backend),
        "insertion_sort" => insertion_sort::emit(params, backend),
        "quick_sort"     => quick_sort::emit(params, backend),
        "merge_sort"     => merge_sort::emit(params, backend),
        "radix_sort"     => radix_sort::emit(params, backend),
        "in"             => fd_in::emit(params, backend),
        "out"            => fd_out::emit(params, backend),
        "open"           => open::emit(params, backend),
        "close"          => close::emit(params, backend),
        "time"           => time::emit(params, backend),
        "args"           => args::emit(params, backend),
        "exit"           => exit::emit(params, backend),
        "env"            => env::emit(params, backend),
        "sleep"          => sleep::emit(params, backend),
        _             => unreachable!(),
    }
}

// ── Shared helpers (private to this module; accessible to all submodules) ─────

fn p(params: &[Param]) -> Vec<&str> {
    params.iter().map(|p| p.name.as_str()).collect()
}

/// Escape a param name that might collide with a Python reserved word.
fn py_esc(name: &str) -> &str {
    match name {
        "from"   => "from_",   "import" => "import_", "class"  => "class_",
        "return" => "return_", "pass"   => "pass_",   "for"    => "for_",
        "while"  => "while_",  "in"     => "in_",     "not"    => "not_",
        "and"    => "and_",    "or"     => "or_",     "if"     => "if_",
        "else"   => "else_",   "lambda" => "lambda_", "with"   => "with_",
        "as"     => "as_",     "try"    => "try_",    "except" => "except_",
        "raise"  => "raise_",  "del"    => "del_",
        other    => other,
    }
}

/// Assert `params` has exactly `n` entries; return their name slices.
fn need<'a>(name: &str, params: &'a [Param], n: usize) -> Result<Vec<&'a str>, String> {
    let v = p(params);
    if v.len() != n {
        return Err(format!(
            "'builtin::{}' requires {} parameter(s) but the function declares {}",
            name, n, v.len()
        ));
    }
    Ok(v)
}
