//! `builtin::i64_to_str(x: i64) -> String`
//!
//! Renamed from `to_string`, which read as a method on an unstated receiver
//! and did not say what it converted.

use bullang::ast::{Backend, Param};
use super::{c_str, CDest, Requirements};

pub const META: &str = "i64_to_str";

pub fn requirements(backend: &Backend) -> Requirements {
    match backend {
        Backend::C   => Requirements::new(c_str::IMPORTS, &[c_str::FT_I64_TO_STR]),
        Backend::Cpp => Requirements::imports(&["#include <string>"]),
        Backend::Go  => Requirements::imports(&["strconv"]),
        _            => Requirements::NONE,
    }
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 1)?;
    let x = p[0];
    Ok(match backend {
        Backend::Rust   => format!("{x}.to_string()"),
        Backend::Python => format!("str({x})"),
        Backend::Cpp    => format!("std::to_string({x})"),
        Backend::Go     => format!("strconv.FormatInt({x}, 10)"),
        Backend::Java   => format!("String.valueOf({x})"),
        Backend::C => return Err(format!(
            "'builtin::{META}' returns a String: the C backend must emit it \
             through stdlib::emit_c_dest, not stdlib::emit_builtin"
        )),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}

pub fn emit_c_dest(dest: &str, params: &[Param]) -> Result<CDest, String> {
    let p = super::need(META, params, 1)?;
    Ok(CDest {
        // Fixed: 20 digits, a sign and a terminator all fit in 24.
        size: "BU_I64_STR_MAX".to_string(),
        call: format!("ft_i64_to_str({}, {})", dest, p[0]),
    })
}
