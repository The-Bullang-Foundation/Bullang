use crate::ast::{Backend, Param};

pub const META: (&str, &str, &str) = (
    "insertion_sort",
    "(arr: Vec[T])             → Vec[T]",
    "Insertion sort — O(n²) stable, in-place over a cloned copy",
);

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need("insertion_sort", params, 1)?;
    let arr = p[0];

    Ok(match backend {
        // ── Rust ─────────────────────────────────────────────────────────────
        // Block expression.  Defines a local fn so recursion / the type bound
        // is expressed cleanly without a closure trait object.
        Backend::Rust => format!(
            "{{\
               fn __isort<T: Ord + Clone>(mut v: Vec<T>) -> Vec<T> {{\
                 let n = v.len();\
                 let mut i = 1usize;\
                 while i < n {{\
                   let key = v[i].clone();\
                   let mut j = i as isize - 1;\
                   while j >= 0 && v[j as usize] > key {{\
                     v[(j + 1) as usize] = v[j as usize].clone();\
                     j -= 1;\
                   }}\
                   v[(j + 1) as usize] = key;\
                   i += 1;\
                 }}\
                 v\
               }}\
               __isort({arr}.clone())\
             }}"
        ),

        // ── Python ───────────────────────────────────────────────────────────
        // Pure recursive lambda (Y-combinator style) — no statements, no exec.
        // Outer lambda: (f, arr, i=1) advances the outer index.
        // Inner lambda: (g, j) shifts elements rightward until the key fits.
        Backend::Python => {
            let arr = super::py_esc(arr);
            format!(
                "(lambda __f: __f(__f, list({arr}), 1))(\
                   lambda __f, __a, __i: __a if __i >= len(__a) else \
                     __f(__f, \
                       (lambda __k: \
                         (lambda __g: __g(__g, __i))(\
                           lambda __g, __j: \
                             [__a.__setitem__(__j, __k)] and __a \
                             if __j == 0 or __a[__j - 1] <= __k \
                             else [__a.__setitem__(__j, __a[__j - 1])] and __g(__g, __j - 1)\
                         )\
                       )(__a[__i]), \
                       __i + 1))"
            )
        }

        // ── C ────────────────────────────────────────────────────────────────
        // GCC compound statement.  Operates on a heap-allocated int64_t[]
        // extracted from the vec_t*.  Returns a new vec_t* (caller owns it).
        // Requires foreign_types.h and <stdlib.h>.
        Backend::C => format!(
            "({{ \
               vec_t *__src = {arr}; \
               size_t __n = vec_len(__src); \
               int64_t *__a = (int64_t *)malloc(__n * sizeof(int64_t)); \
               for (size_t __ii = 0; __ii < __n; __ii++) \
                 __a[__ii] = *(int64_t *)vec_get(__src, __ii); \
               for (size_t __i = 1; __i < __n; __i++) {{ \
                 int64_t __key = __a[__i]; \
                 intptr_t __j = (intptr_t)__i - 1; \
                 while (__j >= 0 && __a[__j] > __key) {{ \
                   __a[__j + 1] = __a[__j]; \
                   __j--; \
                 }} \
                 __a[__j + 1] = __key; \
               }} \
               vec_t *__res = vec_new(); \
               for (size_t __i = 0; __i < __n; __i++) \
                 vec_push(__res, &__a[__i]); \
               free(__a); \
               __res; \
             }})"
        ),

        // ── C++ ──────────────────────────────────────────────────────────────
        // Immediately-invoked lambda; works on a std::vector copy.
        Backend::Cpp => format!(
            "[&]() {{ \
               auto __v = {arr}; \
               auto __n = __v.size(); \
               for (size_t __i = 1; __i < __n; ++__i) {{ \
                 auto __key = __v[__i]; \
                 intptr_t __j = static_cast<intptr_t>(__i) - 1; \
                 while (__j >= 0 && __v[static_cast<size_t>(__j)] > __key) {{ \
                   __v[static_cast<size_t>(__j) + 1] = __v[static_cast<size_t>(__j)]; \
                   --__j; \
                 }} \
                 __v[static_cast<size_t>(__j) + 1] = __key; \
               }} \
               return __v; \
             }}()"
        ),

        // ── Go ───────────────────────────────────────────────────────────────
        // Immediately-invoked function; works on a copy of the slice.
        Backend::Go => format!(
            "func() []int64 {{ \
               __src := {arr}; \
               __v := make([]int64, len(__src)); copy(__v, __src); \
               for __i := 1; __i < len(__v); __i++ {{ \
                 __key := __v[__i]; \
                 __j := __i - 1; \
                 for __j >= 0 && __v[__j] > __key {{ \
                   __v[__j+1] = __v[__j]; \
                   __j--; \
                 }} \
                 __v[__j+1] = __key; \
               }} \
               return __v; \
             }}()"
        ),

        Backend::Unknown(kw) => return Err(format!(
            "'builtin::insertion_sort' is not available for unknown backend '{kw}'"
        )),
    })
}
