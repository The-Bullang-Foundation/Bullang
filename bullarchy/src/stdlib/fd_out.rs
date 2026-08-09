//! `builtin::out(fd: i64, content: String) -> i64`
//!
//! Named fd_out.rs because `out` reads as a direction rather than a file, and
//! to sit beside fd_in.rs. The Bullang name is `builtin::out`.
//!
//! Every backend calls the shim declared in `open.rs`: `fd` is an index into
//! the generated program's own table, not a raw OS descriptor. That is what
//! removed the `write(2)` calls from five backends — they could not build on
//! Windows — and the whole JNI layer from Java.

use bullang::ast::{Backend, Param};
use super::{open, open::Shim, Requirements};

pub const META: &str = "out";

pub fn requirements(backend: &Backend) -> Requirements {
    // The shim is one unit: `out` needs the same table `open` allocates into,
    // so it asks for exactly the same thing.
    open::requirements_for(Shim::Out, backend)
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 2)?;
    let (fd, content) = (p[0], p[1]);
    Ok(match backend {
        Backend::Rust   => format!("bu_out({fd}, &{content})"),
        Backend::Python => format!("bu_out({fd}, {content})"),
        Backend::C      => format!("bu_out({fd}, {content})"),
        Backend::Cpp    => format!("bu_out({fd}, {content})"),
        Backend::Go     => format!("buOut({fd}, {content})"),
        Backend::Java   => format!("BuIo.out({fd}, {content})"),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}
