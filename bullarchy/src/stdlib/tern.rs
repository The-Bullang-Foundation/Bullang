//! `builtin::tern(cond: bool, a: T, b: T) -> T`
//!
//! Takes a condition, not two values to compare. The old four-argument form
//! `tern(v1, v2, a, b)` meaning `v1 == v2 ? a : b` folded a comparison into
//! the call, so the reader had to know the convention to see what was being
//! tested. `(x > 0, "yes", "no")` says it on the line.
//!
//! `T` is `Ty::Same` in the catalogue: whatever the caller passed.

use bullang::ast::{Backend, Param};
use super::Requirements;

pub const META: &str = "tern";

pub fn requirements(backend: &Backend) -> Requirements {
    match backend {
        Backend::Go => Requirements::new(&[], &[GO_HELPER]),
        _           => Requirements::NONE,
    }
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 3)?;
    let (cond, a, b) = (p[0], p[1], p[2]);
    Ok(match backend {
        Backend::Rust   => format!("(if {cond} {{ {a} }} else {{ {b} }})"),
        Backend::Python => format!("({a} if {cond} else {b})"),
        Backend::C      => format!("(({cond}) ? ({a}) : ({b}))"),
        Backend::Cpp    => format!("(({cond}) ? ({a}) : ({b}))"),
        Backend::Java   => format!("(({cond}) ? ({a}) : ({b}))"),
        // Go has no conditional expression, so this is a helper. It is
        // generic, which is what `Ty::Same` needs and what keeps `tern`
        // expression-shaped on all six backends — type inference at the call
        // site means nothing has to be spelled out there.
        Backend::Go     => format!("buTern({cond}, {a}, {b})"),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}

// Generics are Go 1.18; the project already documents a Go 1.21 floor for
// `slices`, so this needs no further note.
const GO_HELPER: &str = r#"func buTern[T any](cond bool, a T, b T) T {
	if cond {
		return a
	}
	return b
}
"#;
