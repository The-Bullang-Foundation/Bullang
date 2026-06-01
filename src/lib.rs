//! Bullang core library.
//!
//! Exports the language definition: grammar, AST, parser, formatter,
//! and stdlib catalogue. Bullarchy and Bullscript depend on this crate.
//! The `bullang` binary (stdlib browsing, update) is a separate target
//! in the same package and does not affect the library's public surface.

pub mod ast;
pub mod checker;
pub mod fmt;
pub mod interpreter;
pub mod parser;
pub mod stdlib;
