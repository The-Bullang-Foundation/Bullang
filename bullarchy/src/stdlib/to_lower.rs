//! `builtin::to_lower(s: String) -> String`

use bullang::ast::{Backend, Param};
use super::{c_str, cpp_str, CDest, Requirements};

pub const META: &str = "to_lower";

pub fn requirements(backend: &Backend) -> Requirements {
    match backend {
        Backend::C   => Requirements::new(c_str::IMPORTS, &[c_str::FT_STRLEN, c_str::FT_TO_LOWER]),
        Backend::Cpp => Requirements::new(cpp_str::IMPORTS, &[cpp_str::BU_TO_LOWER]),
        Backend::Go  => Requirements::imports(&["strings"]),
        _            => Requirements::NONE,
    }
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 1)?;
    let s = p[0];
    Ok(match backend {
        Backend::Rust   => format!("{s}.to_lowercase()"),
        Backend::Python => format!("{s}.lower()"),
        Backend::Cpp    => format!("bu_to_lower({s})"),
        Backend::Go     => format!("strings.ToLower({s})"),
        Backend::Java   => format!("{s}.toLowerCase()"),
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
        size: format!("ft_strlen({}) + 1", p[0]),
        call: format!("ft_to_lower({}, {})", dest, p[0]),
    })
}
