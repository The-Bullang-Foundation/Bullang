//! Bullang core library.
//!
//! Bullang is a language definition, not a runtime. This crate exports the
//! grammar, AST, parser, formatter and the core standard library catalogue —
//! everything needed to read a `.bu` file and understand what it says.
//!
//! Turning that into code is Bullarchy's job: it owns transpilation to the six
//! target backends, project layout, validation and the LSP. Running the result
//! is the target toolchain's job. Bullang itself never executes anything.
//!
//! Packages such as `bull-mathlib` depend on this crate for `ast::{Backend,
//! Param}`, which is the signature every builtin emitter is written against.

pub mod ast;
pub mod fmt;
pub mod parser;
pub mod stdlib;
