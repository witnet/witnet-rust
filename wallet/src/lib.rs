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
//This file is based on wallet/src/lib.rs from
// <https://github.com/mimblewimble/grin>,
// originally developed by The Grin Developers and distributed under the
// Apache License, Version 2.0. You may obtain a copy of the License at
// <http://www.apache.org/licenses/LICENSE-2.0>.

//! Library module for the main wallet functionalities provided by Witnet.

extern crate witnet_core as core;
extern crate witnet_keychain as keychain;
extern crate witnet_util as util;

mod types;

pub use types::{Error};