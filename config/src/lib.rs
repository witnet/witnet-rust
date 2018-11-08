//! # Witnet-rust configuration library.
//!
//! This is the library code for reading and validating the
//! configuration read from an external data source. External data
//! sources and their format are handled through different loaders,
//! see the `witnet_config::loaders` module for more information.
//!
//! No matter which data source you use, ultimately all of them will
//! load the configuration as an instance of the `Config` struct which
//! is composed of other, more specialized, structs such as
//! `StorageConfig` and `ConnectionsConfig`. This instance is the one
//! you use in your Rust code to interact with the loaded
//! configuration.
#![cfg_attr(test, allow(dead_code, unused_macros, unused_imports))]

#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate failure;

use std::default::Default;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

pub mod loaders;

/// The entire configuration
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Config {
    #[serde(default = "Config::default_connections")]
    /// Connections-specific configuration
    pub connections: ConnectionsConfig,
    /// Storage-specific configuration
    #[serde(default = "Config::default_storage")]
    pub storage: StorageConfig,
}

/// Connections-specific configuration
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ConnectionsConfig {
    #[serde(default = "ConnectionsConfig::default_server_addr")]
    /// Server address, that is, the socket address (interface ip and
    /// port) to which the server accepting connections from other
    /// peers should bind to
    pub server_addr: SocketAddr,

    #[serde(default = "ConnectionsConfig::default_inbound_limit")]
    /// Maximum number of concurrent connections the server should
    /// accept
    pub inbound_limit: u16,

    #[serde(default = "ConnectionsConfig::default_outbound_limit")]
    /// Maximum number of opened connections to other peers this node
    /// (acting as a client) should maintain
    pub outbound_limit: u16,

    #[serde(default = "ConnectionsConfig::default_known_peers")]
    /// List of other peer addresses this node knows at start, it is
    /// used as a bootstrap mechanism to gain access to the P2P
    /// network
    pub known_peers: Vec<SocketAddr>,

    #[serde(default = "ConnectionsConfig::default_bootstrap_peers_period")]
    /// Period of the bootstrap peers task (in seconds)
    pub bootstrap_peers_period: Duration,
}

/// Storage-specific configuration
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct StorageConfig {
    #[serde(default = "StorageConfig::default_db_path")]
    /// Path to the directory that will contain the storage-database files
    pub db_path: PathBuf,
}

impl Config {
    /// Default connections-specific configuration
    pub fn default_connections() -> ConnectionsConfig {
        ConnectionsConfig::default()
    }

    /// Default storage-specific configuration
    pub fn default_storage() -> StorageConfig {
        StorageConfig::default()
    }
}

impl Default for Config {
    fn default() -> Config {
        Config {
            connections: Self::default_connections(),
            storage: Self::default_storage(),
        }
    }
}

impl ConnectionsConfig {
    /// Default `server_addr` value (`127.0.0.1:21337`)
    pub fn default_server_addr() -> SocketAddr {
        "127.0.0.1:21337".parse().unwrap()
    }

    /// Default `inbound_limit` value (`128`)
    pub fn default_inbound_limit() -> u16 {
        128
    }

    /// Default `outbound_limit` value (`8`)
    pub fn default_outbound_limit() -> u16 {
        8
    }

    /// Default `known_peers` value (`[]`)
    pub fn default_known_peers() -> Vec<SocketAddr> {
        Vec::default()
    }

    /// Default `bootstrap_peers_period` value (`5`)
    fn default_bootstrap_peers_period() -> Duration {
        Duration::from_secs(5)
    }
}

impl Default for ConnectionsConfig {
    fn default() -> ConnectionsConfig {
        ConnectionsConfig {
            server_addr: Self::default_server_addr(),
            inbound_limit: Self::default_inbound_limit(),
            outbound_limit: Self::default_outbound_limit(),
            known_peers: Self::default_known_peers(),
            bootstrap_peers_period: Self::default_bootstrap_peers_period(),
        }
    }
}

impl StorageConfig {
    /// Default `db_path` value (`.wit`)
    pub fn default_db_path() -> PathBuf {
        PathBuf::from(".wit")
    }
}

impl Default for StorageConfig {
    fn default() -> StorageConfig {
        StorageConfig {
            db_path: Self::default_db_path(),
        }
    }
}
