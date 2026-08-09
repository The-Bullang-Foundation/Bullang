//! `builtin::len(s: String) -> i64`
//!
//! **Characters, not bytes, on every backend.** Rust returned bytes, C
//! returned bytes via `strlen`, Go returned bytes, and Python and Java
//! returned characters — so `len` on any non-ASCII string gave two different
//! answers depending on the target.
//!
//! `len` takes a String only. It used to claim to work on `Vec[T]` as well,
//! but collections were removed: they had a type and no values.

use bullang::ast::{Backend, Param};
use super::{c_str, cpp_str, Requirements};

pub const META: &str = "len";

pub fn requirements(backend: &Backend) -> Requirements {
    match backend {
        Backend::C   => Requirements::new(c_str::IMPORTS, &[c_str::FT_UTF8_LEN]),
        Backend::Cpp => Requirements::new(cpp_str::IMPORTS, &[cpp_str::BU_UTF8_LEN]),
        Backend::Go  => Requirements::imports(&["unicode/utf8"]),
        _            => Requirements::NONE,
    }
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 1)?;
    let s = p[0];
    Ok(match backend {
        Backend::Rust   => format!("({s}.chars().count() as i64)"),
        Backend::Python => format!("len({s})"),
        Backend::C      => format!("ft_utf8_len({s})"),
        Backend::Cpp    => format!("bu_utf8_len({s})"),
        Backend::Go     => format!("int64(utf8.RuneCountInString({s}))"),
        // codePointCount, not length(): length() counts UTF-16 units, so an
        // emoji would count as two.
        Backend::Java   => format!("(long) {s}.codePointCount(0, {s}.length())"),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}
