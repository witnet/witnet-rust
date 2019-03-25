//! crypto

#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

/// Hash functions
pub mod hash;

pub mod cipher;
/// Merkle tree implementation
pub mod merkle;

pub mod key;
/// Cryptographic keys, signatures and mnemonic phrases
pub mod mnemonic;
pub mod pbkdf2;
pub mod signature;
