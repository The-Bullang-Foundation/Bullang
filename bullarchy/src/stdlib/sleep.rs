//! `builtin::sleep(ms: i64)`
//!
//! Milliseconds on every backend. C has no portable millisecond sleep in the
//! standard library, so it gets a helper using `nanosleep`.

use bullang::ast::{Backend, Param};
use super::Requirements;

pub const META: &str = "sleep";

pub fn requirements(backend: &Backend) -> Requirements {
    match backend {
        Backend::Python => Requirements::new(&[], &[PY_HELPER]),
        Backend::C      => Requirements::new(&["#include <time.h>"], &[C_HELPER]),
        Backend::Cpp    => Requirements::imports(&["#include <chrono>", "#include <thread>"]),
        Backend::Go     => Requirements::imports(&["time"]),
        Backend::Java   => Requirements::new(&[], &[JAVA_HELPER]),
        _               => Requirements::NONE,
    }
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 1)?;
    let ms = p[0];
    Ok(match backend {
        Backend::Rust => format!(
            "std::thread::sleep(std::time::Duration::from_millis({ms} as u64))"
        ),
        Backend::Python => format!("bu_sleep({ms})"),
        Backend::C      => format!("bu_sleep({ms})"),
        Backend::Cpp    => format!(
            "std::this_thread::sleep_for(std::chrono::milliseconds({ms}))"
        ),
        Backend::Go     => format!("time.Sleep(time.Duration({ms}) * time.Millisecond)"),
        // Thread.sleep declares a checked exception, so it cannot appear in
        // expression position without a wrapper.
        Backend::Java   => format!("buSleep({ms})"),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}

const PY_HELPER: &str = r#"def bu_sleep(ms):
    __import__("time").sleep(ms / 1000.0)
"#;

const C_HELPER: &str = r#"static void	bu_sleep(long long ms)
{
	struct timespec	ts;

	if (ms < 0)
		return ;
	ts.tv_sec = (time_t)(ms / 1000);
	ts.tv_nsec = (long)((ms % 1000) * 1000000L);
	nanosleep(&ts, NULL);
}
"#;

const JAVA_HELPER: &str = r#"static void buSleep(long ms) {
    try {
        Thread.sleep(ms);
    } catch (InterruptedException e) {
        Thread.currentThread().interrupt();
    }
}
"#;
