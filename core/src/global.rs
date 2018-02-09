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
//This file is based on core/src/global.rs from
// <https://github.com/mimblewimble/grin>,
// originally developed by The Grin Developers and distributed under the
// Apache License, Version 2.0. You may obtain a copy of the License at
// <http://www.apache.org/licenses/LICENSE-2.0>.

//! Values that should be shared across all modules, without necessarily
//! having to pass them all over the place, but aren't consensus values.
//! should be used sparingly.

/// An enum collecting sets of parameters used throughout the
/// code wherever mining is needed. This should allow for
/// different sets of parameters for different purposes,
/// e.g. CI, User testing, production values

use std::sync::RwLock;

// Types of chain a server can run with, dictates the genesis block and
/// and mining parameters used.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChainTypes {
    /// For CI testing
    AutomatedTesting,

    /// For User testing
    UserTesting,

    /// First test network
    Testnet1,
}

impl Default for ChainTypes {
    fn default() -> ChainTypes {
        ChainTypes::UserTesting
    }
}

lazy_static!{
	/// The mining parameter mode
	pub static ref CHAIN_TYPE: RwLock<ChainTypes> = RwLock::new(ChainTypes::Testnet1);
}

/// Set the mining mode
pub fn set_mining_mode(mode: ChainTypes) {
    let mut param_ref = CHAIN_TYPE.write().unwrap();
    *param_ref = mode;
}