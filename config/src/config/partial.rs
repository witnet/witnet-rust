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
use std::collections::HashSet;
use std::default::Default;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use witnet_data_structures::chain::{Environment, Hash};

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

    /// Consensus-critical configuration
    #[serde(default)]
    pub consensus_constants: ConsensusConstants,

    /// JSON-RPC API configuration
    #[serde(default)]
    pub jsonrpc: JsonRPC,
}

/// Connection-specific partial configuration.
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
    #[serde(default)]
    #[serde(deserialize_with = "from_secs")]
    #[serde(rename = "bootstrap_peers_period_seconds")]
    pub bootstrap_peers_period: Option<Duration>,

    /// Period of the persist peers task
    #[serde(default)]
    #[serde(deserialize_with = "from_secs")]
    #[serde(rename = "storage_peers_period_seconds")]
    pub storage_peers_period: Option<Duration>,

    /// Period of the peers discovery task
    #[serde(default)]
    #[serde(deserialize_with = "from_secs")]
    #[serde(rename = "discovery_peers_period_seconds")]
    pub discovery_peers_period: Option<Duration>,

    /// Handshake timeout
    #[serde(default)]
    #[serde(deserialize_with = "from_secs")]
    #[serde(rename = "handshake_timeout_seconds")]
    pub handshake_timeout: Option<Duration>,
}

/// Storage-specific configuration
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
pub struct Storage {
    #[serde(default)]
    /// Path to the directory that will contain the database files
    pub db_path: Option<PathBuf>,
}

/// Consensus-critical configuration
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
pub struct ConsensusConstants {
    /// Timestamp at checkpoint 0 (the start of epoch 0)
    #[serde(default)]
    pub checkpoint_zero_timestamp: Option<i64>,

    /// Seconds between the start of an epoch and the start of the next one
    #[serde(default)]
    #[serde(rename = "checkpoints_period_seconds")]
    pub checkpoints_period: Option<u16>,

    /// Genesis block hash value
    #[serde(default)]
    pub genesis_hash: Option<Hash>,

    /// Decay value for reputation demurrage function
    #[serde(default)]
    pub reputation_demurrage: Option<f64>,

    /// Punishment value for claims out of the consensus bounds
    #[serde(default)]
    pub reputation_punishment: Option<f64>,
}

/// JSON-RPC API configuration
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
pub struct JsonRPC {
    /// Enable JSON-RPC server?
    pub enabled: Option<bool>,
    /// JSON-RPC server address, that is, the socket address (interface ip and
    /// port) for the JSON-RPC server
    pub server_address: Option<SocketAddr>,
}

impl Config {
    pub fn default_mainnet() -> Self {
        let mut default = Config::default();
        default.environment = Environment::Mainnet;
        default
    }
}

use serde::{Deserialize, Deserializer};

// Create a duration type from a u64 representing seconds
fn from_secs<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match u64::deserialize(deserializer) {
        Ok(secs) => Some(Duration::from_secs(secs)),
        Err(_) => None,
    })
}
