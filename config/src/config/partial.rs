//! # Partial Config
//!
//! This module contains the __partial__ `Config` struct. All loaders
//! in `loaders` module will transform the loaded configuration into
//! an instance of this struct. The reason why it is called
//! __partial__ is because some params are optionals and won't be
//! present (they are `None`) if they are not appear in the source,
//! later, the `config` module will use this partial config object and
//! the environment-specific defaults (see the `environment` module)
//! to produce a __total__ (no `Option` fields) configuration object.
use crate::config::Environment;
use std::collections::HashSet;
use std::default::Default;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// The partial configuration object that contains all other, more
/// specific, configuration objects (connections, storage, etc).
#[derive(Deserialize, Default, Debug, PartialEq)]
pub struct Config {
    /// The "environment" in which the protocol will be deployed, eg:
    /// mainnet, testnet, etc.
    #[serde(default)]
    pub environment: Environment,

    /// Connections-related configuration
    #[serde(default)]
    pub connections: Connections,

    /// Storage-related configuration
    #[serde(default)]
    pub storage: Storage,
}

/// Connections-specific partial configuration.
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
pub struct Connections {
    /// Server address, that is, the socket address (interface ip and
    /// port) to which the server accepting connections from other
    /// peers should bind to
    pub server_addr: Option<SocketAddr>,

    /// Maximum number of concurrent connections the server should
    /// accept
    pub inbound_limit: Option<u16>,

    /// Maximum number of opened connections to other peers this node
    /// (acting as a client) should maintain
    pub outbound_limit: Option<u16>,

    /// List of other peer addresses this node knows at start, it is
    /// used as a bootstrap mechanism to gain access to the P2P
    /// network
    #[serde(default)]
    pub known_peers: HashSet<SocketAddr>,

    /// Period of the bootstrap peers task
    pub bootstrap_peers_period: Option<Duration>,

    /// Period of the persist peers task
    pub storage_peers_period: Option<Duration>,

    /// Period of the discovery peers task
    pub discovery_peers_period: Option<Duration>,
}

/// Storage-specific configuration
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
pub struct Storage {
    #[serde(default)]
    /// Path to the directory that will contain the database files
    pub db_path: Option<PathBuf>,
}

impl Default for Environment {
    fn default() -> Environment {
        Environment::Testnet1
    }
}

impl Config {
    pub fn default_mainnet() -> Self {
        let mut default = Config::default();
        default.environment = Environment::Mainnet;
        default
    }
}
