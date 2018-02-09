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
//This file is based on keychain/src/extkey.rs from
// <https://github.com/mimblewimble/grin>,
// originally developed by The Grin Developers and distributed under the
// Apache License, Version 2.0. You may obtain a copy of the License at
// <http://www.apache.org/licenses/LICENSE-2.0>.

use std::{error, fmt, num};

use util::secp;

/// An ExtKey error
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Error {
    /// The size of the seed is invalid
    InvalidSeedSize,
    InvalidSliceSize,
    InvalidExtendedKey,
    Secp(secp::Error),
    ParseIntError(num::ParseIntError),
}

impl From<secp::Error> for Error {
    fn from(e: secp::Error) -> Error {
        Error::Secp(e)
    }
}

impl From<num::ParseIntError> for Error {
    fn from(e: num::ParseIntError) -> Error {
        Error::ParseIntError(e)
    }
}

// Passthrough Debug to Display, since errors should be user-visible
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        f.write_str(error::Error::description(self))
    }
}

impl error::Error for Error {
    fn cause(&self) -> Option<&error::Error> {
        None
    }

    fn description(&self) -> &str {
        match *self {
            Error::InvalidSeedSize => "keychain: seed isn't of size 128, 256 or 512",
            // TODO change when ser. ext. size is fixed
            Error::InvalidSliceSize => "keychain: serialized extended key must be of size 73",
            Error::InvalidExtendedKey => "keychain: the given serialized extended key is invalid",
            Error::Secp(_) => "keychain: secp error",
            Error::ParseIntError(_) => "keychain: error parsing int",
        }
    }
}