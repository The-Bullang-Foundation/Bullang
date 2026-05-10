use crate::ast::{Backend, Param};

pub const META: (&str, &str, &str) = (
    "env",
    "(key: String)             → String",
    "Read an environment variable. Returns an empty String if the key is absent",
);

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need("env", params, 1)?;
    let key = p[0];

    Ok(match backend {
        // ── Rust ─────────────────────────────────────────────────────────────
        Backend::Rust => format!(
            "std::env::var({}.as_str()).unwrap_or_default()",
            key
        ),

        // ── Python ───────────────────────────────────────────────────────────
        Backend::Python => {
            let key = super::py_esc(key);
            format!("__import__('os').environ.get({}, \"\")", key)
        }

        // ── C ────────────────────────────────────────────────────────────────
        // getenv returns NULL on missing key; GCC compound statement returns
        // an empty string in that case.  Requires <stdlib.h>.
        Backend::C => format!(
            "({{ char *__ev = getenv({}); __ev ? __ev : \"\"; }})",
            key
        ),

        // ── C++ ──────────────────────────────────────────────────────────────
        Backend::Cpp => format!(
            "[&]() -> std::string {{ \
               const char *__ev = getenv({}.c_str()); \
               return __ev ? std::string(__ev) : std::string(); \
             }}()",
            key
        ),

        // ── Go ───────────────────────────────────────────────────────────────
        Backend::Go => format!("os.Getenv({})", key),

        Backend::Unknown(kw) => return Err(format!(
            "'builtin::env' is not available for unknown backend '{kw}'"
        )),
    })
}
