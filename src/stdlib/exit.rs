use crate::ast::{Backend, Param};

pub const META: (&str, &str, &str) = (
    "exit",
    "(code: i32)               → (never returns)",
    "Terminate the process with the given exit code",
);

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need("exit", params, 1)?;
    let code = p[0];

    Ok(match backend {
        Backend::Rust   => format!("{{ std::process::exit({} as i32); }}", code),
        Backend::Python => format!("__import__('sys').exit({})", super::py_esc(code)),
        Backend::C      => format!("exit({})", code),
        Backend::Cpp    => format!("std::exit({})", code),
        Backend::Go     => format!("os.Exit(int({}))", code),
        Backend::Unknown(kw) => return Err(format!(
            "'builtin::exit' is not available for unknown backend '{kw}'"
        )),
    })
}
