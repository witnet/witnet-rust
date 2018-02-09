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
//This file is based on grin/src/types.rs from
// <https://github.com/mimblewimble/grin>,
// originally developed by The Grin Developers and distributed under the
// Apache License, Version 2.0. You may obtain a copy of the License at
// <http://www.apache.org/licenses/LICENSE-2.0>.

use chain;
use core::core;
use store;
use core::global::ChainTypes;
use p2p;
use wallet;

/// Error type wrapping underlying module errors.
#[derive(Debug)]
pub enum Error {
    /// Error originating from the core implementation.
    Core(core::block::Error),
    /// Error originating from the db storage.
    Store(store::Error),
    /// Error originating from the blockchain implementation.
    Chain(chain::Error),
    /// Error originating from the peer-to-peer network.
    P2P(p2p::Error),
    /// Error originating from wallet API.
    Wallet(wallet::Error),
}

/// Type of seeding the server will use to find other peers on the network.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Seeding {
    /// No seeding, mostly for tests that programmatically connect
    None,
    /// A list of seed addresses provided to the server
    List,
    /// Automatically download a text file with a list of server addresses
    WebStatic,
    /// Mostly for tests, where connections are initiated programmatically
    Programmatic,
}

impl Default for Seeding {
    fn default() -> Seeding {
        Seeding::None
    }
}

/// Full server configuration, aggregating configurations required for the
/// different components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Directory under which the rocksdb stores will be created
    pub db_root: String,

    /// Which type of chain: tests, testnet or mainnet.
	#[serde(default)]
    pub chain_type: ChainTypes,

    /// Which method will be used to get the list of seed nodes for initial bootstrap.
	#[serde(default)]
	pub seeding_type: Seeding,

    /// Configuration for the peer-to-peer server
    //TODO pub p2p_config: p2p::P2PConfig,

    /// Configuration for the mining daemon
    //TODO pub mining_config: Option<pow::types::MinerConfig>,

    /// Objects pool configuration
    //#[serde(default)]
    //TODO pub pool_config: pool::PoolConfig,

    /// Whether to skip the sync timeout on startup
	/// (To assist testing on solo chains)
    pub skip_sync_wait: Option<bool>,
}

/// Default values for ServerConfig structures.
impl Default for ServerConfig {
    fn default() -> ServerConfig {
        ServerConfig {
            db_root: ".wit".to_string(),
            seeding_type: Seeding::default(),
            //TODO p2p_config: p2p::P2PConfig::default(),
            //TODO mining_config: Some(pow::types::MinerConfig::default()),
            chain_type: ChainTypes::default(),
            //TODO pool_config: PoolConfig::default(),
            skip_sync_wait: Some(true),
        }
    }
}