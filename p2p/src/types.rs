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
//This file is based on p2p/src/types.rs from
// <https://github.com/mimblewimble/grin>,
// originally developed by The Grin Developers and distributed under the
// Apache License, Version 2.0. You may obtain a copy of the License at
// <http://www.apache.org/licenses/LICENSE-2.0>.

use std::io;

use core::core::hash::Hash;
use core::ser;
use store;

#[derive(Debug)]
pub enum Error {
    Serialization(ser::Error),
    Connection(io::Error),
    /// Header type does not match the expected message type
    BadMessage,
    Banned,
    ConnectionClose,
    Timeout,
    Store(store::Error),
    PeerWithSelf,
    ProtocolMismatch {
        us: u32,
        peer: u32,
    },
    GenesisMismatch {
        us: Hash,
        peer: Hash,
    },
}