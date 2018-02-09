//This file is part of Rust-Witnet.
//
//Rust-Witnet is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
//Rust-Witnet is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
//You should have received a copy of the GNU General Public License
// along with Rust-Witnet. If not, see <http://www.gnu.org/licenses/>.
//
//This file is based on core/src/ser.rs from
// <https://github.com/mimblewimble/grin>,
// originally developed by The Grin Developers and distributed under the
// Apache License, Version 2.0. You may obtain a copy of the License at
// <http://www.apache.org/licenses/LICENSE-2.0>.

//! Serialization and deserialization layer specialized for binary encoding.
//! Ensures consistency and safety. Basically a minimal subset or
//! rustc_serialize customized for our need.
//!
//! To use it simply implement `Writeable` or `Readable` and then use the
//! `serialize` or `deserialize` functions on them as appropriate.

use std::fmt;
use std::io::{self, Read, Write};

use consensus;

/// Possible errors deriving from serializing or deserializing.
#[derive(Debug)]
pub enum Error {
    /// Wraps an io error produced when reading or writing
    IOErr(io::Error),
    /// Expected a given value that wasn't found
    UnexpectedData {
        /// What we wanted
        expected: Vec<u8>,
        /// What we got
        received: Vec<u8>,
    },
    /// Data wasn't in a consumable format
    CorruptedData,
    /// When asked to read too much data
    TooLargeReadErr,
    /// Consensus rule failure
    ConsensusError(consensus::Error),
    /// Error from from_hex deserialization
    HexError(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::IOErr(ref e) => write!(f, "{}", e),
            Error::UnexpectedData {
                expected: ref e,
                received: ref r,
            } => write!(f, "expected {:?}, got {:?}", e, r),
            Error::CorruptedData => f.write_str("corrupted data"),
            Error::TooLargeReadErr => f.write_str("too large read"),
            Error::ConsensusError(ref e) => write!(f, "consensus error {:?}", e),
            Error::HexError(ref e) => write!(f, "hex error {:?}", e),
        }
    }
}