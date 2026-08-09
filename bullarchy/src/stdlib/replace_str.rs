//! `builtin::replace_str(s: String, from: String, to: String) -> String`

use bullang::ast::{Backend, Param};
use super::{c_str, cpp_str, CDest, Requirements};

pub const META: &str = "replace_str";

pub fn requirements(backend: &Backend) -> Requirements {
    match backend {
        Backend::C   => Requirements::new(c_str::IMPORTS, &[c_str::FT_STRLEN, c_str::FT_REPLACE]),
        Backend::Cpp => Requirements::new(cpp_str::IMPORTS, &[cpp_str::BU_REPLACE_STR]),
        Backend::Go  => Requirements::imports(&["strings"]),
        _            => Requirements::NONE,
    }
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 3)?;
    let (s, from, to) = (p[0], p[1], p[2]);
    Ok(match backend {
        Backend::Rust   => format!("{s}.replace({from}.as_str(), {to}.as_str())"),
        Backend::Python => format!("{s}.replace({from}, {to})"),
        Backend::Cpp    => format!("bu_replace_str({s}, {from}, {to})"),
        Backend::Go     => format!("strings.ReplaceAll({s}, {from}, {to})"),
        Backend::Java   => format!("{s}.replace({from}, {to})"),
        Backend::C => return Err(format!(
            "'builtin::{META}' returns a String: the C backend must emit it \
             through stdlib::emit_c_dest, not stdlib::emit_builtin"
        )),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}

pub fn emit_c_dest(dest: &str, params: &[Param]) -> Result<CDest, String> {
    let p = super::need(META, params, 3)?;
    let (s, from, to) = (p[0], p[1], p[2]);
    Ok(CDest {
        // Counted exactly rather than guessed: see ft_replace_size.
        size: format!("ft_replace_size({s}, {from}, {to})"),
        call: format!("ft_replace_str({dest}, {s}, {from}, {to})"),
    })
}
