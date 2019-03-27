//! crypto

#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

/// Hash functions
pub mod hash;

/// Merkle tree implementation
pub mod merkle;

/// Cryptographic signatures and mnemonic phrases
pub mod mnemonic;
pub mod signature;
