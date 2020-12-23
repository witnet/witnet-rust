//! Validations

#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

/// Module containing the logic used to update the `ChainState` when consolidating a `Block`
pub mod consolidation;
/// Module containing validations
pub mod validations;

#[cfg(test)]
mod tests;
