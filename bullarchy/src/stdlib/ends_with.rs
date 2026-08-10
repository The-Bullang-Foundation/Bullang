//! `builtin::ends_with(s: String, p: String) -> bool`

use bullang::ast::{Backend, Param};
use super::Requirements;

pub const META: &str = "ends_with";

pub fn requirements(backend: &Backend) -> Requirements {
    match backend {
        Backend::C   => Requirements::new(&["#include <string.h>"], &[C_HELPER]),
        Backend::Cpp => Requirements::new(&["#include <string>"], &[CPP_HELPER]),
        Backend::Go  => Requirements::imports(&["strings"]),
        _            => Requirements::NONE,
    }
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 2)?;
    let (s, suffix) = (p[0], p[1]);
    Ok(match backend {
        Backend::Rust   => format!("{s}.ends_with({suffix}.as_str())"),
        Backend::Python => format!("{s}.endswith({suffix})"),
        Backend::C      => format!("ft_ends_with({s}, {suffix})"),
        Backend::Cpp    => format!("bu_ends_with({s}, {suffix})"),
        Backend::Go     => format!("strings.HasSuffix({s}, {suffix})"),
        Backend::Java   => format!("{s}.endsWith({suffix})"),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}

// A helper rather than an expression because the naive inline form evaluates
// both arguments three times, and one of them may be a function call.
const C_HELPER: &str = r#"static inline int	ft_ends_with(const char *s, const char *suffix)
{
	size_t	slen;
	size_t	xlen;

	slen = strlen(s);
	xlen = strlen(suffix);
	if (xlen > slen)
		return (0);
	return (strcmp(s + slen - xlen, suffix) == 0);
}
"#;

const CPP_HELPER: &str = r#"static bool bu_ends_with(const std::string &s, const std::string &suffix) {
	if (suffix.size() > s.size()) return false;
	return s.compare(s.size() - suffix.size(), suffix.size(), suffix) == 0;
}
"#;
