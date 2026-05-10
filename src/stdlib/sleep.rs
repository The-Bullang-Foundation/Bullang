use crate::ast::{Backend, Param};

pub const META: (&str, &str, &str) = (
    "sleep",
    "(ms: i64)                 → ()",
    "Suspend execution for the given number of milliseconds",
);

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need("sleep", params, 1)?;
    let ms = p[0];

    Ok(match backend {
        // ── Rust ─────────────────────────────────────────────────────────────
        Backend::Rust => format!(
            "std::thread::sleep(std::time::Duration::from_millis({} as u64))",
            ms
        ),

        // ── Python ───────────────────────────────────────────────────────────
        // time.sleep takes seconds as a float.
        Backend::Python => {
            let ms = super::py_esc(ms);
            format!("__import__('time').sleep({} / 1000.0)", ms)
        }

        // ── C ────────────────────────────────────────────────────────────────
        // usleep takes microseconds; requires <unistd.h>.
        // Cast to useconds_t (unsigned int) — safe for sane sleep durations.
        Backend::C => format!("usleep((useconds_t)(({}) * 1000))", ms),

        // ── C++ ──────────────────────────────────────────────────────────────
        // std::this_thread::sleep_for requires <thread> and <chrono>.
        Backend::Cpp => format!(
            "std::this_thread::sleep_for(std::chrono::milliseconds({}))",
            ms
        ),

        // ── Go ───────────────────────────────────────────────────────────────
        // time.Sleep takes a time.Duration (nanoseconds).
        Backend::Go => format!(
            "time.Sleep(time.Duration({}) * time.Millisecond)",
            ms
        ),

        Backend::Unknown(kw) => return Err(format!(
            "'builtin::sleep' is not available for unknown backend '{kw}'"
        )),
    })
}
