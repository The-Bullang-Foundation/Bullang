//! `builtin::in(fd: i64) -> String`
//!
//! Named fd_in.rs because `in` is a Rust keyword and cannot be a module name.
//! The Bullang name is `builtin::in`.
//!
//! Reads one line, newline stripped; an empty string at end of file. Like
//! `out`, it goes through the table declared in `open.rs` — see there for why.

use bullang::ast::{Backend, Param};
use super::{open, open::Shim, CDest, Requirements};

pub const META: &str = "in";

pub fn requirements(backend: &Backend) -> Requirements {
    open::requirements_for(Shim::In, backend)
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 1)?;
    let fd = p[0];
    Ok(match backend {
        Backend::Rust   => format!("bu_in({fd})"),
        Backend::Python => format!("bu_in({fd})"),
        Backend::Cpp    => format!("bu_in({fd})"),
        Backend::Go     => format!("buIn({fd})"),
        Backend::Java   => format!("BuIo.in({fd})"),
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
        // A line has no length known before it is read, so unlike the other
        // string builtins this one has a fixed ceiling rather than a computed
        // size. BU_LINE_MAX is declared with the shim in open.rs.
        size: "BU_LINE_MAX".to_string(),
        call: format!("bu_in({}, {})", dest, p[0]),
    })
}
