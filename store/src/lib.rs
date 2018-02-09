//Rust-Witnet is free software: you can redistribute it and/or modify
//it under the terms of the GNU General Public License as published by
//the Free Software Foundation, either version 3 of the License, or
//(at your option) any later version.
//
//Rust-Witnet is distributed in the hope that it will be useful,
//but WITHOUT ANY WARRANTY; without even the implied warranty of
//MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
//GNU General Public License for more details.
//
//You should have received a copy of the GNU General Public License
//along with Rust-Witnet. If not, see <http://www.gnu.org/licenses/>.
//
//This file is based on store/src/lib.rs from
// <https://github.com/mimblewimble/grin>,
// originally developed by The Grin Developers and distributed under the
// Apache License, Version 2.0. You may obtain a copy of the License at
// <http://www.apache.org/licenses/LICENSE-2.0>.

//! Storage of core types using RocksDB.

#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

extern crate witnet_core as core;

use std::fmt;

use core::ser;

/// Main error type for this crate.
#[derive(Debug)]
pub enum Error {
    /// Couldn't find what we were looking for
    NotFoundErr,
    /// Wraps an error originating from RocksDB (which unfortunately returns
    /// string errors).
    RocksDbErr(String),
    /// Wraps a serialization error for Writeable or Readable
    SerErr(ser::Error),
}

/// Make witnet_store::Error printable.
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &Error::NotFoundErr => write!(f, "Not Found"),
            &Error::RocksDbErr(ref s) => write!(f, "RocksDb Error: {}", s),
            &Error::SerErr(ref e) => write!(f, "Serialization Error: {}", e.to_string()),
        }
    }
}