//! `builtin::env(key: String) -> String`
//!
//! An empty string when the variable is unset — not an error, and not a null
//! the caller has to test for.

use bullang::ast::{Backend, Param};
use super::Requirements;

pub const META: &str = "env";

pub fn requirements(backend: &Backend) -> Requirements {
    match backend {
        Backend::Python => Requirements::new(&[], &[PY_HELPER]),
        Backend::C      => Requirements::new(&["#include <stdlib.h>"], &[C_HELPER]),
        Backend::Cpp    => Requirements::new(&["#include <cstdlib>", "#include <string>"], &[CPP_HELPER]),
        Backend::Go     => Requirements::imports(&["os"]),
        Backend::Java   => Requirements::new(&[], &[JAVA_HELPER]),
        _               => Requirements::NONE,
    }
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 1)?;
    let key = p[0];
    Ok(match backend {
        Backend::Rust   => format!("std::env::var({key}.as_str()).unwrap_or_default()"),
        Backend::Python => format!("bu_env({key})"),
        Backend::C      => format!("bu_env({key})"),
        Backend::Cpp    => format!("bu_env({key})"),
        Backend::Go     => format!("os.Getenv({key})"),
        Backend::Java   => format!("buEnv({key})"),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}

const PY_HELPER: &str = r#"def bu_env(key):
    return __import__("os").environ.get(key, "")
"#;

const C_HELPER: &str = r#"static inline const char	*bu_env(const char *key)
{
	const char	*v;

	v = getenv(key);
	if (!v)
		return ("");
	return (v);
}
"#;

const CPP_HELPER: &str = r#"static std::string bu_env(const std::string &key) {
	const char *v = std::getenv(key.c_str());
	return v ? std::string(v) : std::string();
}
"#;

const JAVA_HELPER: &str = r#"static String buEnv(String key) {
    String v = System.getenv(key);
    return v == null ? "" : v;
}
"#;
