//! `builtin::str_to_i64(s: String) -> i64`
//!
//! Renamed from `parse_i64`, and made to agree across backends: **0 when the
//! string does not parse**, everywhere. Python, C++ and Java used to throw
//! instead, so the same program aborted on three backends and continued on
//! the other three.

use bullang::ast::{Backend, Param};
use super::{cpp_str, Requirements};

pub const META: &str = "str_to_i64";

pub fn requirements(backend: &Backend) -> Requirements {
    match backend {
        Backend::C      => Requirements::imports(&["#include <stdlib.h>"]),
        Backend::Cpp    => Requirements::new(cpp_str::IMPORTS, &[cpp_str::BU_STR_TO_I64]),
        Backend::Go     => Requirements::helper(&[GO_HELPER], &["strconv", "strings"]),
        Backend::Python => Requirements::new(&[], &[PY_HELPER]),
        Backend::Java   => Requirements::new(&[], &[JAVA_HELPER]),
        _               => Requirements::NONE,
    }
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 1)?;
    let s = p[0];
    Ok(match backend {
        Backend::Rust   => format!("{s}.trim().parse::<i64>().unwrap_or(0)"),
        Backend::Python => format!("bu_str_to_i64({s})"),
        // strtoll already returns 0 for a string it cannot parse.
        Backend::C      => format!("strtoll({s}, NULL, 10)"),
        Backend::Cpp    => format!("bu_str_to_i64({s})"),
        Backend::Go     => format!("buStrToI64({s})"),
        Backend::Java   => format!("buStrToI64({s})"),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}

const PY_HELPER: &str = r#"def bu_str_to_i64(s):
    try:
        return int(s.strip())
    except ValueError:
        return 0
"#;

const GO_HELPER: &str = r#"func buStrToI64(s string) int64 {
	n, err := strconv.ParseInt(strings.TrimSpace(s), 10, 64)
	if err != nil {
		return 0
	}
	return n
}
"#;

const JAVA_HELPER: &str = r#"static long buStrToI64(String s) {
    try {
        return Long.parseLong(s.trim());
    } catch (NumberFormatException e) {
        return 0;
    }
}
"#;
