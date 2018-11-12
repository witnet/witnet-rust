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
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::PathBuf;

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
}

/// Connections-specific configuration.
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
}

/// Storage-specific configuration
#[derive(Debug, Clone, PartialEq)]
pub struct Storage {
    /// Path to the directory that will contain the database files
    pub db_path: PathBuf,
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

        Config {
            environment: config.environment.clone(),
            connections: Connections::from_partial(&config.connections, &*defaults),
            storage: Storage::from_partial(&config.storage, &*defaults),
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
        };
        let config = Connections::from_partial(&partial_config, &*defaults);

        assert_eq!(config.server_addr, addr);
        assert_eq!(config.inbound_limit, 3);
        assert_eq!(config.outbound_limit, 4);
        assert!(config.known_peers.contains(&addr));
    }

    #[test]
    fn test_config_default_from_partial() {
        let partial_config = partial::Config::default();
        let config = Config::from_partial(&partial_config);

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
    }
}
