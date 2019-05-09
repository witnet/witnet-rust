// To enable `#[allow(clippy::all)]`
//#![feature(tool_lints)]

#![cfg_attr(test, allow(dead_code, unused_macros, unused_imports))]

#[macro_use]
extern crate protobuf_convert;

/// Module containing functions to generate Witnet's protocol messages
pub mod builders;

/// Module containing Witnet's chain data types
pub mod chain;

/// Module containing functions to convert between Witnet's protocol messages and Protocol Buffers
pub mod proto;

/// Module containing Witnet's protocol messages types
pub mod types;

/// Module containing error definitions
pub mod error;

/// Module containing data_request structures
pub mod data_request;

#[cfg(test)]
pub mod tests;
