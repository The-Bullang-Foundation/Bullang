//! `builtin::argc() -> i64` and `builtin::args(i: i64) -> String`
//!
//! Indexed rather than returning a list: Bullang has no collection type, so
//! the old `args() -> [String]` declared a return value no program could
//! hold. Index 0 is the program name, matching every target language.

use bullang::ast::{Backend, Param};
use super::Requirements;

pub const META: &str = "args";

pub fn requirements(backend: &Backend) -> Requirements {
    match backend {
        Backend::Rust   => Requirements::NONE,
        Backend::Python => Requirements::new(&[], &[PY_HELPER]),
        // C has no ambient argv. `main` stores it; see the C backend's main
        // emitter, which writes bu_argc/bu_argv before anything else runs.
        Backend::C      => Requirements::new(&["#include <stddef.h>"], &[C_HELPER]),
        Backend::Cpp    => Requirements::new(&["#include <string>"], &[CPP_HELPER]),
        Backend::Go     => Requirements::imports(&["os"]),
        Backend::Java   => Requirements::new(&[], &[JAVA_HELPER]),
        Backend::Unknown(_) => Requirements::NONE,
    }
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 1)?;
    let i = p[0];
    Ok(match backend {
        Backend::Rust   => format!("std::env::args().nth({i} as usize).unwrap_or_default()"),
        Backend::Python => format!("bu_args({i})"),
        Backend::C      => format!("bu_args({i})"),
        Backend::Cpp    => format!("bu_args({i})"),
        Backend::Go     => format!("buArgs({i})"),
        Backend::Java   => format!("buArgs({i})"),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}

pub fn emit_argc(params: &[Param], backend: &Backend) -> Result<String, String> {
    super::need("argc", params, 0)?;
    Ok(match backend {
        Backend::Rust   => "(std::env::args().count() as i64)".to_string(),
        Backend::Python => "bu_argc()".to_string(),
        Backend::C      => "((long long)bu_argc)".to_string(),
        Backend::Cpp    => "((long long)bu_argc)".to_string(),
        Backend::Go     => "int64(len(os.Args))".to_string(),
        Backend::Java   => "buArgc()".to_string(),
        Backend::Unknown(_) => return Err(super::unsupported("argc", backend)),
    })
}

const PY_HELPER: &str = r#"def bu_argc():
    return len(_sys.argv)

def bu_args(i):
    if i < 0 or i >= len(_sys.argv):
        return ""
    return _sys.argv[i]
"#;

// argv is captured by main and read through these two globals. An out-of-range
// index returns "" rather than reading past the end, which is what the old
// code did.
const C_HELPER: &str = r#"static int		bu_argc = 0;
static char		**bu_argv = NULL;

static inline const char	*bu_args(long long i)
{
	if (!bu_argv || i < 0 || i >= (long long)bu_argc)
		return ("");
	return (bu_argv[i]);
}
"#;

const CPP_HELPER: &str = r#"static int bu_argc = 0;
static char **bu_argv = nullptr;

static std::string bu_args(long long i) {
	if (!bu_argv || i < 0 || i >= (long long)bu_argc) return "";
	return std::string(bu_argv[i]);
}
"#;

// Java's main receives argv without the program name, so index 0 has to be
// synthesised; the class name is the closest honest equivalent.
const JAVA_HELPER: &str = r#"static String[] buArgv = new String[0];
static String buMainClass = "";

static long buArgc() {
    return buArgv.length + 1L;
}

static String buArgs(long i) {
    if (i == 0) return buMainClass;
    if (i < 1 || i > buArgv.length) return "";
    return buArgv[(int) (i - 1)];
}
"#;
