//! `builtin::time() -> i64`
//!
//! Seconds since the Unix epoch. This is the builtin that motivated moving
//! requirements next to `emit`: it shipped needing `<time.h>` with no entry
//! in the central table, and the generated C did not compile.

use bullang::ast::{Backend, Param};
use super::Requirements;

pub const META: &str = "time";

pub fn requirements(backend: &Backend) -> Requirements {
    match backend {
        Backend::Python => Requirements::new(&[], &[PY_HELPER]),
        Backend::C      => Requirements::imports(&["#include <time.h>"]),
        Backend::Cpp    => Requirements::imports(&["#include <chrono>"]),
        Backend::Go     => Requirements::imports(&["time"]),
        _               => Requirements::NONE,
    }
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    super::need(META, params, 0)?;
    Ok(match backend {
        Backend::Rust => "(std::time::SystemTime::now()\
            .duration_since(std::time::UNIX_EPOCH)\
            .map(|d| d.as_secs() as i64)\
            .unwrap_or(0))".to_string(),
        Backend::Python => "bu_time()".to_string(),
        Backend::C      => "((long long)time(NULL))".to_string(),
        Backend::Cpp    => "((long long)std::chrono::duration_cast<std::chrono::seconds>(\
            std::chrono::system_clock::now().time_since_epoch()).count())".to_string(),
        Backend::Go     => "time.Now().Unix()".to_string(),
        Backend::Java   => "(System.currentTimeMillis() / 1000L)".to_string(),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}

const PY_HELPER: &str = r#"def bu_time():
    return int(__import__("time").time())
"#;
