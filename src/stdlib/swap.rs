use crate::ast::{Backend, Param};

pub const META: (&str, &str, &str) = (
    "swap",
    "(a: T, b: T)              → Tuple[T, T]  (b, a)",
    "Swap two values — returns them in reversed order as a tuple",
);

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need("swap", params, 2)?;
    let (a, b) = (p[0], p[1]);

    Ok(match backend {
        // Block expression: move both into temporaries, return swapped tuple.
        Backend::Rust => format!(
            "{{ let __swap_a = {a}; let __swap_b = {b}; (__swap_b, __swap_a) }}"
        ),

        // Python tuple packing — idiomatic and evaluation-order-safe.
        Backend::Python => {
            let a = super::py_esc(a);
            let b = super::py_esc(b);
            format!("({b}, {a})")
        }

        // GCC compound statement: temp-var swap, evaluates to a struct literal
        // carrying both values.  Requires C99 + GCC/Clang extensions.
        // Note: __typeof__ is a GCC extension; works with all Bullang C targets.
        Backend::C => format!(
            "({{ __typeof__({a}) __sa = ({a}); \
                  __typeof__({b}) __sb = ({b}); \
                  ({a}) = __sb; \
                  ({b}) = __sa; \
                  ({a}); }})"
        ),

        // Immediately-invoked lambda; captures by value, returns std::pair.
        Backend::Cpp => format!(
            "[&]() {{ auto __sa = {a}; auto __sb = {b}; \
               return std::make_pair(__sb, __sa); }}()"
        ),

        // Immediately-invoked function; returns the two values as a Go
        // multi-return pair using interface{} so the type is preserved at
        // the call site through type assertion.
        Backend::Go => format!(
            "func() (interface{{}}, interface{{}}) \
               {{ __sa := {a}; __sb := {b}; return __sb, __sa }}()"
        ),

        Backend::Unknown(kw) => return Err(format!(
            "'builtin::swap' is not available for unknown backend '{kw}'"
        )),
    })
}
