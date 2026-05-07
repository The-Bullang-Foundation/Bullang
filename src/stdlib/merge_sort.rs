use crate::ast::{Backend, Param};

pub const META: (&str, &str, &str) = (
    "merge_sort",
    "(arr: Vec[T])             → Vec[T]",
    "Merge sort — O(n log n) stable, returns a sorted copy",
);

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need("merge_sort", params, 1)?;
    let arr = p[0];

    Ok(match backend {
        // ── Rust ─────────────────────────────────────────────────────────────
        // Top-down merge sort via a local fn.  The merge step uses two
        // indices into temporary left/right vecs to avoid O(n) slicing.
        Backend::Rust => format!(
            "{{\
               fn __msort<T: Ord + Clone>(v: Vec<T>) -> Vec<T> {{\
                 if v.len() <= 1 {{ return v; }}\
                 let mid = v.len() / 2;\
                 let left  = __msort(v[..mid].to_vec());\
                 let right = __msort(v[mid..].to_vec());\
                 let (mut li, mut ri) = (0, 0);\
                 let mut out = Vec::with_capacity(left.len() + right.len());\
                 while li < left.len() && ri < right.len() {{\
                   if left[li] <= right[ri] {{ out.push(left[li].clone());  li += 1; }}\
                   else                     {{ out.push(right[ri].clone()); ri += 1; }}\
                 }}\
                 out.extend_from_slice(&left[li..]);\
                 out.extend_from_slice(&right[ri..]);\
                 out\
               }}\
               __msort({arr}.clone())\
             }}"
        ),

        // ── Python ───────────────────────────────────────────────────────────
        // Recursive lambda — top-down merge sort + merge expressed as a nested
        // recursive lambda (Y-combinator for the merge step too).
        // No calls to sorted(), list.sort(), or any Python sort primitive.
        Backend::Python => {
            let arr = super::py_esc(arr);
            format!(
                "(lambda __f: __f(__f, list({arr})))(\
                   lambda __f, __a: __a if len(__a) <= 1 else \
                     (lambda __l, __r: \
                       (lambda __m: __m(__m, __l, __r, []))(\
                         lambda __m, __l, __r, __acc: \
                           __acc + __l + __r if not __l or not __r \
                           else __m(__m, __l[1:], __r, __acc + [__l[0]]) if __l[0] <= __r[0] \
                           else __m(__m, __l, __r[1:], __acc + [__r[0]])\
                       )\
                     )(  \
                       __f(__f, __a[:len(__a)//2]), \
                       __f(__f, __a[len(__a)//2:]) \
                     )\
                 )"
            )
        }

        // ── C ────────────────────────────────────────────────────────────────
        // GCC compound statement — bottom-up iterative merge sort on an
        // int64_t[] copy (avoids recursion overhead and stack limits in C).
        Backend::C => format!(
            "({{ \
               vec_t *__src = {arr}; \
               size_t __n = vec_len(__src); \
               int64_t *__a = (int64_t *)malloc(__n * sizeof(int64_t)); \
               int64_t *__b = (int64_t *)malloc(__n * sizeof(int64_t)); \
               for (size_t __ii = 0; __ii < __n; __ii++) \
                 __a[__ii] = *(int64_t *)vec_get(__src, __ii); \
               for (size_t __w = 1; __w < __n; __w *= 2) {{ \
                 for (size_t __lo = 0; __lo < __n; __lo += 2 * __w) {{ \
                   size_t __mid = __lo + __w < __n ? __lo + __w : __n; \
                   size_t __hi  = __lo + 2*__w < __n ? __lo + 2*__w : __n; \
                   size_t __i = __lo, __j = __mid, __k = __lo; \
                   while (__i < __mid && __j < __hi) \
                     __b[__k++] = __a[__i] <= __a[__j] ? __a[__i++] : __a[__j++]; \
                   while (__i < __mid) __b[__k++] = __a[__i++]; \
                   while (__j < __hi)  __b[__k++] = __a[__j++]; \
                 }} \
                 int64_t *__tmp = __a; __a = __b; __b = __tmp; \
               }} \
               vec_t *__res = vec_new(); \
               for (size_t __i = 0; __i < __n; __i++) \
                 vec_push(__res, &__a[__i]); \
               free(__a); free(__b); \
               __res; \
             }})"
        ),

        // ── C++ ──────────────────────────────────────────────────────────────
        // Immediately-invoked lambda; top-down merge sort on a std::vector
        // copy via a recursive std::function.
        Backend::Cpp => format!(
            "[&]() {{ \
               using __V = std::vector<int64_t>; \
               std::function<__V(__V)> __msort = [&](__V __v) -> __V {{ \
                 if (__v.size() <= 1) return __v; \
                 size_t __mid = __v.size() / 2; \
                 __V __l = __msort(__V(__v.begin(), __v.begin() + __mid)); \
                 __V __r = __msort(__V(__v.begin() + __mid, __v.end())); \
                 __V __out; __out.reserve(__v.size()); \
                 size_t __li = 0, __ri = 0; \
                 while (__li < __l.size() && __ri < __r.size()) \
                   __out.push_back(__l[__li] <= __r[__ri] ? __l[__li++] : __r[__ri++]); \
                 __out.insert(__out.end(), __l.begin() + __li, __l.end()); \
                 __out.insert(__out.end(), __r.begin() + __ri, __r.end()); \
                 return __out; \
               }}; \
               return __msort({arr}); \
             }}()"
        ),

        // ── Go ───────────────────────────────────────────────────────────────
        // Immediately-invoked function; top-down merge sort on a slice copy.
        Backend::Go => format!(
            "func() []int64 {{ \
               var __msort func([]int64) []int64; \
               __msort = func(__v []int64) []int64 {{ \
                 if len(__v) <= 1 {{ return __v }} \
                 __mid := len(__v) / 2; \
                 __l := __msort(append([]int64{{}}, __v[:__mid]...)); \
                 __r := __msort(append([]int64{{}}, __v[__mid:]...)); \
                 __out := make([]int64, 0, len(__v)); \
                 __li, __ri := 0, 0; \
                 for __li < len(__l) && __ri < len(__r) {{ \
                   if __l[__li] <= __r[__ri] {{ \
                     __out = append(__out, __l[__li]); __li++; \
                   }} else {{ \
                     __out = append(__out, __r[__ri]); __ri++; \
                   }} \
                 }} \
                 __out = append(__out, __l[__li:]...); \
                 __out = append(__out, __r[__ri:]...); \
                 return __out; \
               }}; \
               __cp := make([]int64, len({arr})); copy(__cp, {arr}); \
               return __msort(__cp); \
             }}()"
        ),

        Backend::Unknown(kw) => return Err(format!(
            "'builtin::merge_sort' is not available for unknown backend '{kw}'"
        )),
    })
}
