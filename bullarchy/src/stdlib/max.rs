//! `builtin::max(a: i64, b: i64) -> i64`
//!
//! Two integers, not an array — see `min`.

use bullang::ast::{Backend, Param};
use super::Requirements;

pub const META: &str = "max";

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
        Backend::Rust   => format!("std::cmp::max({a}, {b})"),
        Backend::Python => format!("max({a}, {b})"),
        Backend::C      => format!("(({a}) > ({b}) ? ({a}) : ({b}))"),
        Backend::Cpp    => format!("std::max<long long>({a}, {b})"),
        Backend::Go     => format!("max({a}, {b})"),
        Backend::Java   => format!("Math.max({a}, {b})"),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}
