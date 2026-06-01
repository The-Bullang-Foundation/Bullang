//! Bullang standard library catalogue.
//!
//! Defines the full set of universal builtins available in every backend.
//! Emit logic lives in Bullarchy, which owns the transpiler pipeline.
//! Bullang exposes the catalogue for `bullang stdlib --list`.

/// Full builtin catalogue: (name, signature, description).
pub const BUILTINS: &[(&str, &str, &str)] = &[
    // math
    ("abs",            "abs(x: i64) -> i64",                        "absolute value"),
    ("pow",            "pow(base: i64, exp: i64) -> i64",            "integer exponentiation"),
    ("powf",           "powf(base: f64, exp: f64) -> f64",           "floating-point exponentiation"),
    ("sqrt",           "sqrt(x: f64) -> f64",                        "square root"),
    ("clamp",          "clamp(x: i64, lo: i64, hi: i64) -> i64",     "clamp x between lo and hi"),
    ("min",            "min(a: i64, b: i64) -> i64",                 "minimum of two integers"),
    ("max",            "max(a: i64, b: i64) -> i64",                 "maximum of two integers"),
    ("log",            "log(x: f64, base: f64) -> f64",              "logarithm"),
    ("exp",            "exp(x: f64) -> f64",                         "e raised to the power x"),
    // conditions
    ("tern",           "tern(cond: bool, a: T, b: T) -> T",          "ternary — returns a if cond, else b"),
    // string
    ("to_upper",       "to_upper(s: String) -> String",              "uppercase"),
    ("to_lower",       "to_lower(s: String) -> String",              "lowercase"),
    ("trim",           "trim(s: String) -> String",                  "strip leading/trailing whitespace"),
    ("starts_with",    "starts_with(s: String, p: String) -> bool",  "prefix test"),
    ("ends_with",      "ends_with(s: String, p: String) -> bool",    "suffix test"),
    ("replace_str",    "replace_str(s: String, from: String, to: String) -> String", "replace all occurrences"),
    ("to_string",      "to_string(x: i64) -> String",                "integer to string"),
    ("parse_i64",      "parse_i64(s: String) -> i64",                "parse integer from string"),
    ("len",            "len(s: String) -> i64",                      "string or array length"),
    // algorithms
    ("swap",           "swap(a: T, b: T) -> (T, T)",                 "swap two values"),
    ("insertion_sort", "insertion_sort(arr: [i64]) -> [i64]",        "insertion sort"),
    ("quick_sort",     "quick_sort(arr: [i64]) -> [i64]",            "quicksort"),
    ("merge_sort",     "merge_sort(arr: [i64]) -> [i64]",            "merge sort"),
    ("radix_sort",     "radix_sort(arr: [i64]) -> [i64]",            "radix sort"),
    // io
    ("in",             "in(fd: i32) -> String",                      "read one line from a file descriptor"),
    ("out",            "out(fd: i32, content: String) -> i32",       "write a string to a file descriptor, returns bytes written"),
    ("open",           "open(path: String, mode: String) -> i64",    "open a file, returns fd"),
    ("close",          "close(fd: i64)",                             "close a file descriptor"),
    ("time",           "time() -> i64",                              "unix timestamp in seconds"),
    // system
    ("args",           "args() -> [String]",                         "command-line arguments"),
    ("exit",           "exit(code: i64)",                            "exit with code"),
    ("env",            "env(key: String) -> String",                 "read environment variable"),
    ("sleep",          "sleep(ms: i64)",                             "sleep for ms milliseconds"),
];

/// Return the full builtin catalogue.
pub fn list_builtins() -> Vec<(&'static str, &'static str, &'static str)> {
    BUILTINS.to_vec()
}
