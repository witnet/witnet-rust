//! # Config
//!
//! This module contains the `Config` struct, which holds all the
//! configuration params for Witnet. The `Config` struct in this
//! module is __total__, that is, it contains all the required fields
//! needed by the rest of the application unlike the partial
//! [Config](config::partial::Config) which is
//! __partial__, meaning most fields are optional and they may not
//! appear in configuration file in which case a default value for the
//! environment will be used.
//!
//! All the [loaders](loaders) will always return a partial
//! configuration but you shouldn't use that one directly but the one
//! in this module and use the method: `Config::from_partial`, to
//! obtain a total config objects from a partial one.
//!
//! You can create an instance of this config in serveral ways:
//!
//! * By creating the instance manually:
//! ```
//! // Config { environment: Environment::Testnet1, ... }
//! ```
//! * By using the [Default](std::default::Default) instance
//! ```
//! use witnet_config::config::Config;
//!
//! Config::default();
//! ```
//! * By using a partial [Config](config::partial::Config) instance
//!   that will be merged on top of the environment-specific one
//!   ([defaults](defaults))
//! ```
//! use witnet_config::config::{partial, Config};
//!
//! // Default config for testnet
//! Config::from_partial(&partial::Config::default());
//!
//! // Default config for mainnet
//! // Config::from_partial(&partial::Config::default_mainnet());
//! ```

use crate::defaults::{Defaults, Testnet1};
use log::warn;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Module containing the partial configuration struct that is
/// returned by the loaders.
pub mod partial;

/// The total configuration object that contains all other, more
/// specific, configuration objects (connections, storage, etc).
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// The "environment" in which the protocol will be deployed, eg:
    /// mainnet, testnet, etc.
    pub environment: Environment,

    /// Connections-related configuration
    pub connections: Connections,

    /// Storage-related configuration
    pub storage: Storage,

    /// Consensus-critical configuration
    pub consensus_constants: ConsensusConstants,

    /// JSON-RPC API configuration
    pub jsonrpc: JsonRPC,
}

/// Connection-specific configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct Connections {
    /// Server address, that is, the socket address (interface ip and
    /// port) to which the server accepting connections from other
    /// peers should bind to
    pub server_addr: SocketAddr,

    /// Maximum number of concurrent connections the server should
    /// accept
    pub inbound_limit: u16,

    /// Maximum number of opened connections to other peers this node
    /// (acting as a client) should maintain
    pub outbound_limit: u16,

    /// List of other peer addresses this node knows at start, it is
    /// used as a bootstrap mechanism to gain access to the P2P
    /// network
    pub known_peers: HashSet<SocketAddr>,

    /// Period of the bootstrap peers task
    pub bootstrap_peers_period: Duration,

    /// Period of the persist peers task
    pub storage_peers_period: Duration,

    /// Period of the peers discovery task
    pub discovery_peers_period: Duration,

    /// Handshake timeout
    pub handshake_timeout: Duration,
}

/// Storage-specific configuration
#[derive(Debug, Clone, PartialEq)]
pub struct Storage {
    /// Path to the directory that will contain the database files
    pub db_path: PathBuf,
}

/// Consensus-critical configuration
#[derive(Debug, Clone, PartialEq)]
pub struct ConsensusConstants {
    /// Timestamp at checkpoint 0 (the start of epoch 0)
    pub checkpoint_zero_timestamp: i64,

    /// Seconds between the start of an epoch and the start of the next one
    pub checkpoints_period: u16,
}

/// JsonRPC API configuration
#[derive(Debug, Clone, PartialEq)]
pub struct JsonRPC {
    /// JSON-RPC server address, that is, the socket address (interface ip and
    /// port) for the JSON-RPC server
    pub server_address: SocketAddr,
}

/// Possible values for the "environment" configuration param.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub enum Environment {
    /// "mainnet" environment
    #[serde(rename = "mainnet")]
    Mainnet,
    /// "testnet" environment
    #[serde(rename = "testnet-1")]
    Testnet1,
}

impl Config {
    pub fn from_partial(config: &partial::Config) -> Self {
        let defaults: Box<Defaults> = match config.environment {
            Environment::Mainnet => {
                panic!("Config with mainnet environment is currently not allowed");
            }
            Environment::Testnet1 => Box::new(Testnet1),
        };

        let consensus_constants = match config.environment {
            // When in mainnet, ignore the [consensus_constants] section of the configuration
            Environment::Mainnet => {
                let consensus_constants_no_changes = partial::ConsensusConstants::default();
                // Warn the user if the config file contains a non-empty [consensus_constant] section
                if config.consensus_constants != consensus_constants_no_changes {
                    warn!(
                        "Consensus constants in the configuration are ignored when running mainnet"
                    );
                }
                ConsensusConstants::from_partial(&consensus_constants_no_changes, &*defaults)
            }
            // In testnet, allow to override the consensus constants
            Environment::Testnet1 => {
                ConsensusConstants::from_partial(&config.consensus_constants, &*defaults)
            }
        };

        Config {
            environment: config.environment.clone(),
            connections: Connections::from_partial(&config.connections, &*defaults),
            storage: Storage::from_partial(&config.storage, &*defaults),
            consensus_constants,
            jsonrpc: JsonRPC::from_partial(&config.jsonrpc, &*defaults),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::from_partial(&partial::Config::default())
    }
}

impl Connections {
    pub fn from_partial(config: &partial::Connections, defaults: &Defaults) -> Self {
        Connections {
            server_addr: config
                .server_addr
                .to_owned()
                .unwrap_or_else(|| defaults.connections_server_addr()),
            inbound_limit: config
                .inbound_limit
                .to_owned()
                .unwrap_or_else(|| defaults.connections_inbound_limit()),
            outbound_limit: config
                .outbound_limit
                .to_owned()
                .unwrap_or_else(|| defaults.connections_outbound_limit()),
            known_peers: config
                .known_peers
                .union(&defaults.connections_known_peers())
                .cloned()
                .collect(),
            bootstrap_peers_period: config
                .bootstrap_peers_period
                .to_owned()
                .unwrap_or_else(|| defaults.connections_bootstrap_peers_period()),
            storage_peers_period: config
                .storage_peers_period
                .to_owned()
                .unwrap_or_else(|| defaults.connections_storage_peers_period()),
            discovery_peers_period: config
                .discovery_peers_period
                .to_owned()
                .unwrap_or_else(|| defaults.connections_discovery_peers_period()),
            handshake_timeout: config
                .handshake_timeout
                .unwrap_or_else(|| defaults.connections_handshake_timeout()),
        }
    }
}

impl Storage {
    pub fn from_partial(config: &partial::Storage, defaults: &Defaults) -> Self {
        Storage {
            db_path: config
                .db_path
                .to_owned()
                .unwrap_or_else(|| defaults.storage_db_path()),
        }
    }
}

impl ConsensusConstants {
    pub fn from_partial(config: &partial::ConsensusConstants, defaults: &dyn Defaults) -> Self {
        ConsensusConstants {
            checkpoint_zero_timestamp: config
                .checkpoint_zero_timestamp
                .to_owned()
                .unwrap_or_else(|| defaults.consensus_constants_checkpoint_zero_timestamp()),
            checkpoints_period: config
                .checkpoint_period
                .to_owned()
                .unwrap_or_else(|| defaults.consensus_constants_checkpoints_period()),
        }
    }
}

impl JsonRPC {
    pub fn from_partial(config: &partial::JsonRPC, defaults: &dyn Defaults) -> Self {
        JsonRPC {
            server_address: config
                .server_address
                .to_owned()
                .unwrap_or_else(|| defaults.jsonrpc_server_address()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_default_from_partial() {
        let defaults: Box<Defaults> = Box::new(Testnet1);
        let partial_config = partial::Storage::default();
        let config = Storage::from_partial(&partial_config, &*defaults);

        assert_eq!(config.db_path.to_str(), Testnet1.storage_db_path().to_str());
    }

    #[test]
    fn test_storage_from_partial() {
        let defaults: Box<Defaults> = Box::new(Testnet1);
        let partial_config = partial::Storage {
            db_path: Some(PathBuf::from("other")),
        };
        let config = Storage::from_partial(&partial_config, &*defaults);

        assert_eq!(config.db_path.to_str(), Some("other"));
    }

    #[test]
    fn test_connections_default_from_partial() {
        let defaults: Box<Defaults> = Box::new(Testnet1);
        let partial_config = partial::Connections::default();
        let config = Connections::from_partial(&partial_config, &*defaults);

        assert_eq!(config.server_addr, Testnet1.connections_server_addr());
        assert_eq!(config.inbound_limit, Testnet1.connections_inbound_limit());
        assert_eq!(config.outbound_limit, Testnet1.connections_outbound_limit());
        assert_eq!(config.known_peers, Testnet1.connections_known_peers());
        assert_eq!(
            config.bootstrap_peers_period,
            Testnet1.connections_bootstrap_peers_period()
        );
        assert_eq!(
            config.storage_peers_period,
            Testnet1.connections_storage_peers_period()
        );
        assert_eq!(
            config.discovery_peers_period,
            Testnet1.connections_discovery_peers_period()
        );
        assert_eq!(
            config.handshake_timeout,
            Testnet1.connections_handshake_timeout()
        );
    }

    #[test]
    fn test_connections_from_partial() {
        let defaults: Box<Defaults> = Box::new(Testnet1);
        let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();
        let partial_config = partial::Connections {
            server_addr: Some(addr),
            inbound_limit: Some(3),
            outbound_limit: Some(4),
            known_peers: [addr].iter().cloned().collect(),
            bootstrap_peers_period: Some(Duration::from_secs(10)),
            storage_peers_period: Some(Duration::from_secs(60)),
            discovery_peers_period: Some(Duration::from_secs(100)),
            handshake_timeout: Some(Duration::from_secs(3)),
        };
        let config = Connections::from_partial(&partial_config, &*defaults);

        assert_eq!(config.server_addr, addr);
        assert_eq!(config.inbound_limit, 3);
        assert_eq!(config.outbound_limit, 4);
        assert!(config.known_peers.contains(&addr));
        assert_eq!(config.bootstrap_peers_period, Duration::from_secs(10));
        assert_eq!(config.storage_peers_period, Duration::from_secs(60));
        assert_eq!(config.discovery_peers_period, Duration::from_secs(100));
        assert_eq!(config.handshake_timeout, Duration::from_secs(3));
    }

    #[test]
    fn test_config_default_from_partial() {
        let partial_config = partial::Config::default();
        let config = Config::from_partial(&partial_config);

        assert_eq!(config.environment, Environment::Testnet1);
        assert_eq!(
            config.connections.server_addr,
            Testnet1.connections_server_addr()
        );
        assert_eq!(
            config.connections.inbound_limit,
            Testnet1.connections_inbound_limit()
        );
        assert_eq!(
            config.connections.outbound_limit,
            Testnet1.connections_outbound_limit()
        );
        assert_eq!(
            config.connections.known_peers,
            Testnet1.connections_known_peers()
        );
        assert_eq!(
            config.connections.bootstrap_peers_period,
            Testnet1.connections_bootstrap_peers_period()
        );
        assert_eq!(
            config.connections.storage_peers_period,
            Testnet1.connections_storage_peers_period()
        );
        assert_eq!(
            config.connections.discovery_peers_period,
            Testnet1.connections_discovery_peers_period()
        );
        assert_eq!(
            config.connections.handshake_timeout,
            Testnet1.connections_handshake_timeout()
        );
        assert_eq!(config.storage.db_path, Testnet1.storage_db_path());
    }
}
