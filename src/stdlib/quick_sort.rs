use crate::ast::{Backend, Param};

pub const META: (&str, &str, &str) = (
    "quick_sort",
    "(arr: Vec[T])             → Vec[T]",
    "Quicksort — O(n log n) average, Lomuto partition, returns a sorted copy",
);

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need("quick_sort", params, 1)?;
    let arr = p[0];

    Ok(match backend {
        // ── Rust ─────────────────────────────────────────────────────────────
        // Lomuto-style quicksort via a local fn (allows recursion without
        // boxing the closure).
        Backend::Rust => format!(
            "{{\
               fn __qsort<T: Ord + Clone>(v: Vec<T>) -> Vec<T> {{\
                 if v.len() <= 1 {{ return v; }}\
                 let pivot = v[0].clone();\
                 let left:  Vec<T> = v[1..].iter().filter(|x| **x <= pivot).cloned().collect();\
                 let right: Vec<T> = v[1..].iter().filter(|x| **x >  pivot).cloned().collect();\
                 let mut out = __qsort(left);\
                 out.push(pivot);\
                 out.extend(__qsort(right));\
                 out\
               }}\
               __qsort({arr}.clone())\
             }}"
        ),

        // ── Python ───────────────────────────────────────────────────────────
        // Recursive lambda — pivot is first element (Lomuto style).
        // Clean single expression; no calls to sorted() or list.sort().
        Backend::Python => {
            let arr = super::py_esc(arr);
            format!(
                "(lambda __f: __f(__f, list({arr})))(\
                   lambda __f, __a: [] if not __a else \
                     __f(__f, [__x for __x in __a[1:] if __x <= __a[0]]) + \
                     [__a[0]] + \
                     __f(__f, [__x for __x in __a[1:] if __x > __a[0]]))"
            )
        }

        // ── C ────────────────────────────────────────────────────────────────
        // GCC compound statement.  Implements iterative quicksort via an
        // explicit stack (avoids stack-overflow on large inputs).
        // Operates on a heap-allocated int64_t[] copy of the vec_t*.
        Backend::C => format!(
            "({{ \
               vec_t *__src = {arr}; \
               size_t __n = vec_len(__src); \
               int64_t *__a = (int64_t *)malloc(__n * sizeof(int64_t)); \
               for (size_t __ii = 0; __ii < __n; __ii++) \
                 __a[__ii] = *(int64_t *)vec_get(__src, __ii); \
               if (__n > 1) {{ \
                 intptr_t __stk[128][2]; int __top = 0; \
                 __stk[__top][0] = 0; __stk[__top][1] = (intptr_t)__n - 1; __top++; \
                 while (__top > 0) {{ \
                   --__top; \
                   intptr_t __lo = __stk[__top][0], __hi = __stk[__top][1]; \
                   if (__lo >= __hi) continue; \
                   int64_t __piv = __a[__hi]; intptr_t __pi = __lo - 1; \
                   for (intptr_t __jj = __lo; __jj < __hi; __jj++) \
                     if (__a[__jj] <= __piv) {{ \
                       __pi++; \
                       int64_t __t = __a[__pi]; __a[__pi] = __a[__jj]; __a[__jj] = __t; \
                     }} \
                   __pi++; \
                   int64_t __t = __a[__pi]; __a[__pi] = __a[__hi]; __a[__hi] = __t; \
                   __stk[__top][0] = __lo;       __stk[__top][1] = __pi - 1; __top++; \
                   __stk[__top][0] = __pi + 1;   __stk[__top][1] = __hi;     __top++; \
                 }} \
               }} \
               vec_t *__res = vec_new(); \
               for (size_t __i = 0; __i < __n; __i++) \
                 vec_push(__res, &__a[__i]); \
               free(__a); \
               __res; \
             }})"
        ),

        // ── C++ ──────────────────────────────────────────────────────────────
        // Immediately-invoked lambda; defines a recursive std::function
        // inside (Lomuto partition on a std::vector copy).
        Backend::Cpp => format!(
            "[&]() {{ \
               std::function<std::vector<int64_t>(std::vector<int64_t>)> __qsort = \
                 [&](std::vector<int64_t> __v) -> std::vector<int64_t> {{ \
                   if (__v.size() <= 1) return __v; \
                   int64_t __piv = __v[0]; \
                   std::vector<int64_t> __left, __right; \
                   for (size_t __i = 1; __i < __v.size(); ++__i) \
                     (__v[__i] <= __piv ? __left : __right).push_back(__v[__i]); \
                   auto __out = __qsort(__left); \
                   __out.push_back(__piv); \
                   auto __r = __qsort(__right); \
                   __out.insert(__out.end(), __r.begin(), __r.end()); \
                   return __out; \
                 }}; \
               return __qsort({arr}); \
             }}()"
        ),

        // ── Go ───────────────────────────────────────────────────────────────
        // Immediately-invoked function; Lomuto partition on a slice copy.
        Backend::Go => format!(
            "func() []int64 {{ \
               var __qsort func([]int64) []int64; \
               __qsort = func(__v []int64) []int64 {{ \
                 if len(__v) <= 1 {{ return __v }} \
                 __piv := __v[0]; \
                 var __left, __right []int64; \
                 for _, __x := range __v[1:] {{ \
                   if __x <= __piv {{ __left = append(__left, __x) }} else {{ __right = append(__right, __x) }} \
                 }} \
                 __out := __qsort(__left); \
                 __out  = append(__out, __piv); \
                 __out  = append(__out, __qsort(__right)...); \
                 return __out; \
               }}; \
               __cp := make([]int64, len({arr})); copy(__cp, {arr}); \
               return __qsort(__cp); \
             }}()"
        ),

        Backend::Unknown(kw) => return Err(format!(
            "'builtin::quick_sort' is not available for unknown backend '{kw}'"
        )),
    })
}
