//! `builtin::close(fd: i64)`
//!
//! Closes a descriptor from the table in `open.rs`. Closing 0, 1 or 2 does
//! nothing: the standard streams outlive the program's own bookkeeping, and
//! the old code closed them for real, which broke any later `out` to stdout.

use bullang::ast::{Backend, Param};
use super::{open, open::Shim, Requirements};

pub const META: &str = "close";

pub fn requirements(backend: &Backend) -> Requirements {
    open::requirements_for(Shim::Close, backend)
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 1)?;
    let fd = p[0];
    Ok(match backend {
        Backend::Rust   => format!("bu_close({fd})"),
        Backend::Python => format!("bu_close({fd})"),
        Backend::C      => format!("bu_close({fd})"),
        Backend::Cpp    => format!("bu_close({fd})"),
        Backend::Go     => format!("buClose({fd})"),
        Backend::Java   => format!("BuIo.close({fd})"),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}
