//! `builtin::exit(code: i64)`

use bullang::ast::{Backend, Param};
use super::Requirements;

pub const META: &str = "exit";

pub fn requirements(backend: &Backend) -> Requirements {
    match backend {
        Backend::C   => Requirements::imports(&["#include <stdlib.h>"]),
        Backend::Cpp => Requirements::imports(&["#include <cstdlib>"]),
        Backend::Go  => Requirements::imports(&["os"]),
        _            => Requirements::NONE,
    }
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 1)?;
    let code = p[0];
    Ok(match backend {
        Backend::Rust   => format!("std::process::exit({code} as i32)"),
        Backend::Python => format!("_sys.exit({code})"),
        Backend::C      => format!("exit((int)({code}))"),
        Backend::Cpp    => format!("std::exit((int)({code}))"),
        Backend::Go     => format!("os.Exit(int({code}))"),
        Backend::Java   => format!("System.exit((int)({code}))"),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}
