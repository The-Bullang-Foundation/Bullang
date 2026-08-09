//! `builtin::to_upper(s: String) -> String`

use bullang::ast::{Backend, Param};
use super::{c_str, cpp_str, CDest, Requirements};

pub const META: &str = "to_upper";

pub fn requirements(backend: &Backend) -> Requirements {
    match backend {
        Backend::C   => Requirements::new(c_str::IMPORTS, &[c_str::FT_STRLEN, c_str::FT_TO_UPPER]),
        Backend::Cpp => Requirements::new(cpp_str::IMPORTS, &[cpp_str::BU_TO_UPPER]),
        Backend::Go  => Requirements::imports(&["strings"]),
        _            => Requirements::NONE,
    }
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 1)?;
    let s = p[0];
    Ok(match backend {
        Backend::Rust   => format!("{s}.to_uppercase()"),
        Backend::Python => format!("{s}.upper()"),
        Backend::Cpp    => format!("bu_to_upper({s})"),
        Backend::Go     => format!("strings.ToUpper({s})"),
        Backend::Java   => format!("{s}.toUpperCase()"),
        // C goes through emit_c_dest — the backend must not reach here.
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
        call: format!("ft_to_upper({}, {})", dest, p[0]),
    })
}
