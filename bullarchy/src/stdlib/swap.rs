//! `builtin::swap(a: T, b: T) -> Tuple[T, T]` — returns `(b, a)`.
//!
//! In the catalogue as `Ty::Tuple(&[Ty::Same, Ty::Same])`, which is what
//! `Ty::Tuple` was added for: `Tuple[T, T]` cannot be spelled with `Ty::Same`
//! alone. Each backend delegates to whatever its own language already has for
//! a pair — a Rust tuple, a `std::pair`, a Python tuple, a `java.util.List`,
//! and the named tuple struct C and Go generate.

use bullang::ast::{Backend, BuType, Param};
use super::Requirements;

pub const META: &str = "swap";

pub fn requirements(backend: &Backend) -> Requirements {
    match backend {
        Backend::Cpp  => Requirements::imports(&["#include <utility>"]),
        Backend::Java => Requirements::new(&[], &[JAVA_HELPER]),
        _             => Requirements::NONE,
    }
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 2)?;
    let (a, b) = (p[0], p[1]);
    Ok(match backend {
        // A bullet's inputs are identifiers or literals — never calls, since
        // one operation per bullet is a grammar rule — so naming each side
        // twice cannot evaluate anything twice. That is what lets all six of
        // these be plain expressions instead of the block, statement
        // expression and lambda wrappers this used to emit.
        Backend::Rust   => format!("({b}, {a})"),
        Backend::Python => format!("({b}, {a})"),
        Backend::Cpp    => format!("std::make_pair({b}, {a})"),
        Backend::Java   => format!("buSwap({a}, {b})"),
        Backend::C => {
            let name = tuple_name(params, backend)?;
            format!("(({name}){{ .v0 = {b}, .v1 = {a} }})")
        }
        Backend::Go => {
            let name = tuple_name(params, backend)?;
            format!("{name}{{V0: {b}, V1: {a}}}")
        }
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}

/// C and Go represent `Tuple[T, U]` as a named struct, so both need `T`
/// concretely to know which struct to build.
fn tuple_name(params: &[Param], backend: &Backend) -> Result<String, String> {
    let ty = &params[0].ty;
    if matches!(ty, BuType::Unknown) {
        return Err(format!(
            "'builtin::swap' could not determine the type of '{}' for the '{}' \
             backend, which needs it to name the Tuple[T, T] struct it returns",
            params[0].name,
            backend.name()
        ));
    }
    let pair = [ty.clone(), ty.clone()];
    Ok(match backend {
        Backend::C => crate::codegen::codegen_c::tuple_c_name(&pair),
        _          => crate::codegen::codegen_go::tuple_go_name(&pair),
    })
}

// `Object[]`, not `List`, because that is how the Java backend already
// represents `Tuple[A, B]` — a helper returning anything else does not fit the
// declared return type of the function calling it.
const JAVA_HELPER: &str = r#"static <T> Object[] buSwap(T a, T b) {
    return new Object[] { b, a };
}
"#;
