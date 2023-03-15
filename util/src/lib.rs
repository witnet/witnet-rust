//! The `util` package contains useful structs, traits, types, etc. that can be easily used across
//! all the Witnet-rust project.

#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

/// Utilities to securely store secrets in files
pub mod files;

/// Timestamp as UTC
pub mod timestamp;
