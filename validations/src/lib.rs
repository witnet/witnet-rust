// To enable `#[allow(clippy::all)]`
//#![feature(tool_lints)]

#![cfg_attr(test, allow(dead_code, unused_macros, unused_imports))]

/// Module containing validations
pub mod validations;

#[cfg(test)]
mod tests;
