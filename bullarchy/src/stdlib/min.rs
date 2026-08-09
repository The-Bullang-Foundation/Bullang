//! `builtin::min(a: i64, b: i64) -> i64`
//!
//! Two integers, not an array: collections were removed, so the old
//! `min(arr: Vec[T])` had a signature no program could satisfy.

use bullang::ast::{Backend, Param};
use super::Requirements;

pub const META: &str = "min";

pub fn requirements(backend: &Backend) -> Requirements {
    match backend {
        Backend::Cpp => Requirements::imports(&["#include <algorithm>"]),
        _            => Requirements::NONE,
    }
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 2)?;
    let (a, b) = (p[0], p[1]);
    Ok(match backend {
        Backend::Rust   => format!("std::cmp::min({a}, {b})"),
        Backend::Python => format!("min({a}, {b})"),
        // Parenthesised because the arguments may be expressions, and
        // evaluated once each by binding them first would need a statement —
        // these are pure integer expressions, so the conditional is safe.
        Backend::C      => format!("(({a}) < ({b}) ? ({a}) : ({b}))"),
        Backend::Cpp    => format!("std::min<long long>({a}, {b})"),
        Backend::Go     => format!("min({a}, {b})"),
        Backend::Java   => format!("Math.min({a}, {b})"),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}
