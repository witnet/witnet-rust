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

use std::default::Default;
use std::net::SocketAddr;
use std::path::PathBuf;

pub mod loaders;

/// The entire configuration
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Config {
    #[serde(default = "Config::default_connections")]
    pub connections: ConnectionsConfig,
    #[serde(default = "Config::default_storage")]
    pub storage: StorageConfig,
}

/// Connections-specific configuration
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ConnectionsConfig {
    #[serde(default = "ConnectionsConfig::default_server_addr")]
    pub server_addr: SocketAddr,
    #[serde(default = "ConnectionsConfig::default_inbound_limit")]
    pub inbound_limit: u16,
    #[serde(default = "ConnectionsConfig::default_outbound_limit")]
    pub outbound_limit: u16,
    #[serde(default = "ConnectionsConfig::default_known_peers")]
    pub known_peers: Vec<SocketAddr>,
}

/// Storage-specific configuration
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct StorageConfig {
    #[serde(default = "StorageConfig::default_db_path")]
    pub db_path: PathBuf,
}

impl Config {
    fn default_connections() -> ConnectionsConfig {
        ConnectionsConfig::default()
    }

    fn default_storage() -> StorageConfig {
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
    fn default_server_addr() -> SocketAddr {
        "127.0.0.1:1234".parse().unwrap()
    }

    fn default_inbound_limit() -> u16 {
        256
    }

    fn default_outbound_limit() -> u16 {
        8
    }

    fn default_known_peers() -> Vec<SocketAddr> {
        Vec::default()
    }
}

impl Default for ConnectionsConfig {
    fn default() -> ConnectionsConfig {
        ConnectionsConfig {
            server_addr: Self::default_server_addr(),
            inbound_limit: Self::default_inbound_limit(),
            outbound_limit: Self::default_outbound_limit(),
            known_peers: Self::default_known_peers(),
        }
    }
}

impl StorageConfig {
    fn default_db_path() -> PathBuf {
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
