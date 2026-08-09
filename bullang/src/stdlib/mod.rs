//! Bullang standard library catalogue.
//!
//! Bullang's core is deliberately small. Anything beyond it — maths, sorting,
//! networking — lives in a separate package installed with `bullarchy add` and
//! declared with `#use:`. That is why `abs`, `sqrt`, `pow` and the sorts are
//! not here: they are `bull-mathlib`'s, not core's.
//!
//! This crate holds only the catalogue — the name, signature and description of
//! each builtin. Emitting code for them belongs to Bullarchy, which owns the
//! transpiler, and to each package for its own builtins.
//!
//! The category lives in the data rather than in a hand-written list beside it.
//! The previous arrangement kept six separate name arrays in the CLI, and they
//! drifted: eleven names were printed as core long after they had moved to
//! mathlib, and the display code silently skipped anything it could not find,
//! so `bullang stdlib` quietly showed a Math section containing only `min` and
//! `max`.

use crate::ast::BuType;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Conditions,
    Math,
    String,
    Io,
    System,
}

impl Category {
    /// Display order for `bullang stdlib`.
    pub const ALL: &'static [Category] = &[
        Category::Math,
        Category::Conditions,
        Category::String,
        Category::Io,
        Category::System,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            Category::Math       => "Math",
            Category::Conditions => "Conditions",
            Category::String     => "String",
            Category::Io         => "I/O",
            Category::System     => "System",
        }
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.title())
    }
}

/// One builtin in the catalogue.
///
/// `params` and `returns` are the machine-readable types; `signature` is the
/// human-readable rendering shown by `bullang stdlib`. Both are given rather
/// than deriving one from the other: a type checker should never depend on
/// parsing a string written for a reader.
pub struct Builtin {
    pub name:        &'static str,
    pub signature:   &'static str,
    pub description: &'static str,
    pub category:    Category,
    pub params:      &'static [Ty],
    pub returns:     Ty,
}

/// A builtin parameter or return type.
///
/// `Same` is how the catalogue expresses a builtin that works across types
/// without Bullang having generics: `tern(cond: bool, a: T, b: T) -> T` is
/// `[Bool, Same, Same]` returning `Same`, meaning "whatever the caller passed".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    I64,
    F64,
    Bool,
    Str,
    Unit,
    Same,
    /// A tuple of the types given. Exists so `swap` can say what it returns:
    /// `Tuple(&[Same, Same])` is `Tuple[T, T]`, the pair the caller passed in
    /// the other order. Nested `Same` is resolved by the call site, the same
    /// way a bare `Same` is.
    Tuple(&'static [Ty]),
}

impl Ty {
    /// The concrete Bullang type, or None when the call site is needed to
    /// resolve it — a bare `Same`, or a tuple containing one.
    pub fn to_butype(self) -> Option<BuType> {
        self.resolve(None)
    }

    /// The concrete Bullang type, with `Same` standing for `same`.
    ///
    /// `same` is whatever the caller passed in the builtin's interchangeable
    /// positions; `None` means the call site could not determine it either, in
    /// which case an unresolved `Same` stays unresolved.
    pub fn resolve(self, same: Option<&BuType>) -> Option<BuType> {
        let name = match self {
            Ty::I64  => "i64",
            Ty::F64  => "f64",
            Ty::Bool => "bool",
            Ty::Str  => "String",
            Ty::Unit => "()",
            Ty::Same => return same.cloned(),
            Ty::Tuple(inner) => {
                let mut parts = Vec::with_capacity(inner.len());
                for t in inner {
                    parts.push(t.resolve(same)?);
                }
                return Some(BuType::Tuple(parts));
            }
        };
        Some(BuType::Named(name.to_string()))
    }
}

/// The full core builtin catalogue.
pub const BUILTINS: &[Builtin] = &[
    // ── math ──────────────────────────────────────────────────────────────
    Builtin { name: "min", signature: "min(a: i64, b: i64) -> i64",
              description: "smaller of two integers", category: Category::Math,
              params: &[Ty::I64, Ty::I64], returns: Ty::I64 },
    Builtin { name: "max", signature: "max(a: i64, b: i64) -> i64",
              description: "larger of two integers", category: Category::Math,
              params: &[Ty::I64, Ty::I64], returns: Ty::I64 },

    // ── conditions ────────────────────────────────────────────────────────
    Builtin { name: "tern", signature: "tern(cond: bool, a: T, b: T) -> T",
              description: "returns a if cond, else b", category: Category::Conditions,
              params: &[Ty::Bool, Ty::Same, Ty::Same], returns: Ty::Same },
    // Value plumbing with no domain of its own, so it sits beside `tern`
    // rather than under Math or System.
    Builtin { name: "swap", signature: "swap(a: T, b: T) -> Tuple[T, T]",
              description: "the same two values, in the other order",
              category: Category::Conditions,
              params: &[Ty::Same, Ty::Same],
              returns: Ty::Tuple(&[Ty::Same, Ty::Same]) },

    // ── string ────────────────────────────────────────────────────────────
    Builtin { name: "to_upper", signature: "to_upper(s: String) -> String",
              description: "uppercase", category: Category::String,
              params: &[Ty::Str], returns: Ty::Str },
    Builtin { name: "to_lower", signature: "to_lower(s: String) -> String",
              description: "lowercase", category: Category::String,
              params: &[Ty::Str], returns: Ty::Str },
    Builtin { name: "trim", signature: "trim(s: String) -> String",
              description: "strip leading and trailing whitespace", category: Category::String,
              params: &[Ty::Str], returns: Ty::Str },
    Builtin { name: "starts_with", signature: "starts_with(s: String, p: String) -> bool",
              description: "prefix test", category: Category::String,
              params: &[Ty::Str, Ty::Str], returns: Ty::Bool },
    Builtin { name: "ends_with", signature: "ends_with(s: String, p: String) -> bool",
              description: "suffix test", category: Category::String,
              params: &[Ty::Str, Ty::Str], returns: Ty::Bool },
    Builtin { name: "replace_str", signature: "replace_str(s: String, from: String, to: String) -> String",
              description: "replace every occurrence", category: Category::String,
              params: &[Ty::Str, Ty::Str, Ty::Str], returns: Ty::Str },
    Builtin { name: "i64_to_str", signature: "i64_to_str(x: i64) -> String",
              description: "integer to string", category: Category::String,
              params: &[Ty::I64], returns: Ty::Str },
    Builtin { name: "str_to_i64", signature: "str_to_i64(s: String) -> i64",
              description: "string to integer, 0 if it does not parse", category: Category::String,
              params: &[Ty::Str], returns: Ty::I64 },
    Builtin { name: "len", signature: "len(s: String) -> i64",
              description: "length in characters", category: Category::String,
              params: &[Ty::Str], returns: Ty::I64 },

    // ── io ────────────────────────────────────────────────────────────────
    // A file descriptor is an index into a table the generated program keeps,
    // not a raw OS descriptor: 0, 1 and 2 are stdin/stdout/stderr, and `open`
    // allocates from 3 up. That is what lets the same program work on Windows,
    // where native handles are not integers.
    Builtin { name: "in", signature: "in(fd: i64) -> String",
              description: "read one line from a file descriptor", category: Category::Io,
              params: &[Ty::I64], returns: Ty::Str },
    Builtin { name: "out", signature: "out(fd: i64, content: String) -> i64",
              description: "write a string to a file descriptor, returns bytes written",
              category: Category::Io,
              params: &[Ty::I64, Ty::Str], returns: Ty::I64 },
    Builtin { name: "open", signature: "open(path: String, mode: String) -> i64",
              description: "open a file in mode r, w, a or rw; returns a descriptor",
              category: Category::Io,
              params: &[Ty::Str, Ty::Str], returns: Ty::I64 },
    Builtin { name: "close", signature: "close(fd: i64)",
              description: "close a file descriptor", category: Category::Io,
              params: &[Ty::I64], returns: Ty::Unit },
    Builtin { name: "time", signature: "time() -> i64",
              description: "unix timestamp in seconds", category: Category::Io,
              params: &[], returns: Ty::I64 },

    // ── system ────────────────────────────────────────────────────────────
    // `args` is indexed rather than returning a collection: Bullang has no
    // collection type, so a list could be declared but never built.
    Builtin { name: "argc", signature: "argc() -> i64",
              description: "number of command-line arguments", category: Category::System,
              params: &[], returns: Ty::I64 },
    Builtin { name: "args", signature: "args(i: i64) -> String",
              description: "command-line argument at index i", category: Category::System,
              params: &[Ty::I64], returns: Ty::Str },
    Builtin { name: "exit", signature: "exit(code: i64)",
              description: "exit with a status code", category: Category::System,
              params: &[Ty::I64], returns: Ty::Unit },
    Builtin { name: "env", signature: "env(key: String) -> String",
              description: "read an environment variable", category: Category::System,
              params: &[Ty::Str], returns: Ty::Str },
    Builtin { name: "sleep", signature: "sleep(ms: i64)",
              description: "sleep for a number of milliseconds", category: Category::System,
              params: &[Ty::I64], returns: Ty::Unit },
    Builtin { name: "run", signature: "run(cmd: String) -> i64",
              description: "run a shell command, returns its exit code",
              category: Category::System,
              params: &[Ty::Str], returns: Ty::I64 },
];

/// Every builtin in the catalogue.
pub fn list_builtins() -> &'static [Builtin] {
    BUILTINS
}

/// Every builtin in one category, in catalogue order.
pub fn by_category(category: Category) -> impl Iterator<Item = &'static Builtin> {
    BUILTINS.iter().filter(move |b| b.category == category)
}

/// Look a builtin up by name.
pub fn find(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.name == name)
}

/// True if `name` is a core builtin. Package builtins are not included —
/// Bullarchy resolves those against whatever the project has installed.
pub fn is_core_builtin(name: &str) -> bool {
    find(name).is_some()
}
