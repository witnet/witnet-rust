//! Witnet storage module that conveniently abstracts a key/value API away from specific storage
//! backends.
#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

#[macro_use]
extern crate log;

pub mod error;
pub mod backends;
pub mod storage;