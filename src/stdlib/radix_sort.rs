use crate::ast::{Backend, Param};

pub const META: (&str, &str, &str) = (
    "radix_sort",
    "(arr: Vec[i64])           → Vec[i64]",
    "Radix sort (LSD, base-256) — O(kn) stable, signed integers, returns a sorted copy",
);

// Radix sort strategy: LSD, base 256 (4 passes over 32-bit range, 8 over 64-bit).
// Signed integer handling: offset by min so all values become non-negative before
// sorting, then subtract the offset from each result.

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need("radix_sort", params, 1)?;
    let arr = p[0];

    Ok(match backend {
        // ── Rust ─────────────────────────────────────────────────────────────
        // LSD radix sort over i64 values.  Shifts by min so all values are
        // non-negative, runs 8 byte-passes (base 256), then restores offset.
        Backend::Rust => format!(
            "{{\
               fn __rsort(mut v: Vec<i64>) -> Vec<i64> {{\
                 if v.len() <= 1 {{ return v; }}\
                 let min = *v.iter().min().unwrap();\
                 if min < 0 {{ for x in v.iter_mut() {{ *x = x.wrapping_sub(min); }} }}\
                 let mut buf = vec![0i64; v.len()];\
                 for __shift in (0u32..64).step_by(8) {{\
                   let mut cnt = [0usize; 256];\
                   for &x in &v {{ cnt[((x >> __shift) & 0xff) as usize] += 1; }}\
                   let mut pre = 0usize;\
                   for c in cnt.iter_mut() {{ let t = *c; *c = pre; pre += t; }}\
                   for &x in &v {{\
                     let b = ((x >> __shift) & 0xff) as usize;\
                     buf[cnt[b]] = x; cnt[b] += 1;\
                   }}\
                   std::mem::swap(&mut v, &mut buf);\
                 }}\
                 if min < 0 {{ for x in v.iter_mut() {{ *x = x.wrapping_add(min); }} }}\
                 v\
               }}\
               __rsort({arr}.clone())\
             }}"
        ),

        // ── Python ───────────────────────────────────────────────────────────
        // Pure recursive lambda.  Uses base 10 (clearer for the expression
        // form) with the same min-offset trick for signed values.
        // No calls to sorted() or list.sort().
        Backend::Python => {
            let arr = super::py_esc(arr);
            format!(
                "(lambda __raw: \
                   (lambda __min, __a: \
                     (lambda __f: \
                       [__x + __min for __x in \
                         __f(__f, __a, max(__a) if __a else 0, 1)] \
                     )(\
                       lambda __f, __a, __m, __exp: __a if __exp > __m else \
                         __f(__f, \
                           [__x for __b in \
                             [[__x for __x in __a if (__x // __exp) % 10 == __d] \
                              for __d in range(10)] \
                           for __x in __b], \
                           __m, __exp * 10) \
                     ) \
                   )(\
                     min(__raw) if __raw else 0, \
                     [__x - (min(__raw) if __raw else 0) for __x in __raw] \
                   ) \
                 )(list({arr}))"
            )
        }

        // ── C ────────────────────────────────────────────────────────────────
        // GCC compound statement.  LSD radix sort in base 256 on a
        // heap-allocated int64_t[] copy.  Signed handling via uint64_t
        // offset (add UINT64_MAX/2+1 to map signed range onto unsigned).
        Backend::C => format!(
            "({{ \
               vec_t *__src = {arr}; \
               size_t __n = vec_len(__src); \
               uint64_t *__a = (uint64_t *)malloc(__n * sizeof(uint64_t)); \
               uint64_t *__b = (uint64_t *)malloc(__n * sizeof(uint64_t)); \
               const uint64_t __off = (uint64_t)1 << 63; \
               for (size_t __ii = 0; __ii < __n; __ii++) \
                 __a[__ii] = (uint64_t)(*(int64_t *)vec_get(__src, __ii)) + __off; \
               for (int __sh = 0; __sh < 64; __sh += 8) {{ \
                 size_t __cnt[256] = {{0}}; \
                 for (size_t __i = 0; __i < __n; __i++) \
                   __cnt[(__a[__i] >> __sh) & 0xff]++; \
                 size_t __pre = 0; \
                 for (int __d = 0; __d < 256; __d++) {{ \
                   size_t __t = __cnt[__d]; __cnt[__d] = __pre; __pre += __t; \
                 }} \
                 for (size_t __i = 0; __i < __n; __i++) {{ \
                   int __d = (__a[__i] >> __sh) & 0xff; \
                   __b[__cnt[__d]++] = __a[__i]; \
                 }} \
                 uint64_t *__tmp = __a; __a = __b; __b = __tmp; \
               }} \
               vec_t *__res = vec_new(); \
               for (size_t __i = 0; __i < __n; __i++) {{ \
                 int64_t __v = (int64_t)(__a[__i] - __off); \
                 vec_push(__res, &__v); \
               }} \
               free(__a); free(__b); \
               __res; \
             }})"
        ),

        // ── C++ ──────────────────────────────────────────────────────────────
        // Immediately-invoked lambda; LSD radix sort in base 256 on a
        // std::vector<int64_t> copy.  Same signed offset trick as C.
        Backend::Cpp => format!(
            "[&]() {{ \
               auto __raw = {arr}; \
               size_t __n = __raw.size(); \
               if (__n <= 1) return __raw; \
               const uint64_t __off = uint64_t(1) << 63; \
               std::vector<uint64_t> __a(__n), __b(__n); \
               for (size_t __i = 0; __i < __n; ++__i) \
                 __a[__i] = static_cast<uint64_t>(__raw[__i]) + __off; \
               for (int __sh = 0; __sh < 64; __sh += 8) {{ \
                 size_t __cnt[256] = {{}}; \
                 for (auto __x : __a) __cnt[(__x >> __sh) & 0xff]++; \
                 size_t __pre = 0; \
                 for (auto &__c : __cnt) {{ size_t __t = __c; __c = __pre; __pre += __t; }} \
                 for (auto __x : __a) {{ int __d = (__x >> __sh) & 0xff; __b[__cnt[__d]++] = __x; }} \
                 std::swap(__a, __b); \
               }} \
               std::vector<int64_t> __res(__n); \
               for (size_t __i = 0; __i < __n; ++__i) \
                 __res[__i] = static_cast<int64_t>(__a[__i] - __off); \
               return __res; \
             }}()"
        ),

        // ── Go ───────────────────────────────────────────────────────────────
        // Immediately-invoked function; LSD radix sort in base 256.
        // Maps int64 → uint64 via +offset before sorting, reverses after.
        Backend::Go => format!(
            "func() []int64 {{ \
               __raw := {arr}; \
               __n := len(__raw); \
               if __n <= 1 {{ \
                 __cp := make([]int64, __n); copy(__cp, __raw); return __cp; \
               }} \
               const __off = uint64(1) << 63; \
               __a := make([]uint64, __n); \
               __b := make([]uint64, __n); \
               for __i, __v := range __raw {{ __a[__i] = uint64(__v) + __off }} \
               for __sh := uint(0); __sh < 64; __sh += 8 {{ \
                 var __cnt [256]int; \
                 for _, __x := range __a {{ __cnt[(__x>>__sh)&0xff]++ }} \
                 __pre := 0; \
                 for __d := range __cnt {{ __t := __cnt[__d]; __cnt[__d] = __pre; __pre += __t }} \
                 for _, __x := range __a {{ \
                   __d := (__x >> __sh) & 0xff; __b[__cnt[__d]] = __x; __cnt[__d]++; \
                 }} \
                 __a, __b = __b, __a; \
               }} \
               __res := make([]int64, __n); \
               for __i, __x := range __a {{ __res[__i] = int64(__x - __off) }} \
               return __res; \
             }}()"
        ),

        Backend::Unknown(kw) => return Err(format!(
            "'builtin::radix_sort' is not available for unknown backend '{kw}'"
        )),
    })
}
