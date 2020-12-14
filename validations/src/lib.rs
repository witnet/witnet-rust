// To enable `#[allow(clippy::all)]`
//#![feature(tool_lints)]

/// Module containing validations
pub mod validations;

/// Module containing post mainnet validations
pub mod mainnet_validations;

#[cfg(test)]
mod tests;
