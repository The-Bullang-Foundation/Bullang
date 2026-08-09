//! `builtin::starts_with(s: String, p: String) -> bool`

use bullang::ast::{Backend, Param};
use super::Requirements;

pub const META: &str = "starts_with";

pub fn requirements(backend: &Backend) -> Requirements {
    match backend {
        Backend::C   => Requirements::imports(&["#include <string.h>"]),
        Backend::Cpp => Requirements::imports(&["#include <string>"]),
        Backend::Go  => Requirements::imports(&["strings"]),
        _            => Requirements::NONE,
    }
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 2)?;
    let (s, prefix) = (p[0], p[1]);
    Ok(match backend {
        Backend::Rust   => format!("{s}.starts_with({prefix}.as_str())"),
        Backend::Python => format!("{s}.startswith({prefix})"),
        Backend::C      => format!("(strncmp({s}, {prefix}, strlen({prefix})) == 0)"),
        Backend::Cpp    => format!("({s}.rfind({prefix}, 0) == 0)"),
        Backend::Go     => format!("strings.HasPrefix({s}, {prefix})"),
        Backend::Java   => format!("{s}.startsWith({prefix})"),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}
