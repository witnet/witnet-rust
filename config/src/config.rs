//! # Config
//!
//! This module contains the `Config` struct, which holds all the
//! configuration params for Witnet. The `Config` struct in this
//! module is __total__, that is, it contains all the required fields
//! needed by the rest of the application unlike the partial
//! [Config](config::PartialConfig) which is
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
//! ```text
//! Config { environment: Environment::Testnet1, ... }
//! ```
//! * By using the [Default](std::default::Default) instance
//! ```
//! use witnet_config::config::Config;
//!
//! Config::default();
//! ```
//! * By using a partial [Config](config::PartialConfig) instance
//!   that will be merged on top of the environment-specific one
//!   ([defaults](defaults))
//! ```
//! use witnet_config::config::{PartialConfig, Config};
//!
//! // Default config for testnet
//! Config::from_partial(&PartialConfig::default());
//!
//! // Default config for mainnet
//! // Config::from_partial(&PartialConfig::default_mainnet());
//! ```
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use log::warn;
use serde::{Deserialize, Deserializer, Serialize};

use crate::defaults::{Defaults, Testnet1, Testnet3};
use crate::dirs;
use partial_struct::PartialStruct;
use witnet_crypto::hash::HashFunction;
use witnet_data_structures::chain::{ConsensusConstants, Environment, PartialConsensusConstants};
use witnet_protected::{Protected, ProtectedString};

/// The total configuration object that contains all other, more
/// specific, configuration objects (connections, storage, etc).
#[derive(PartialStruct, Debug, Clone, PartialEq)]
#[partial_struct(derive(Deserialize, Default, Debug, Clone, PartialEq))]
pub struct Config {
    /// The "environment" in which the protocol will be deployed, eg:
    /// mainnet, testnet, etc.
    #[partial_struct(skip)]
    #[partial_struct(serde(default))]
    pub environment: Environment,

    /// Connections-related configuration
    #[partial_struct(ty = "PartialConnections")]
    #[partial_struct(serde(default))]
    pub connections: Connections,

    /// Storage-related configuration
    #[partial_struct(ty = "PartialStorage")]
    #[partial_struct(serde(default))]
    pub storage: Storage,

    /// Consensus-critical configuration
    #[partial_struct(ty = "PartialConsensusConstants")]
    #[partial_struct(serde(default))]
    pub consensus_constants: ConsensusConstants,

    /// JSON-RPC API configuration
    #[partial_struct(ty = "PartialJsonRPC")]
    #[partial_struct(serde(default))]
    pub jsonrpc: JsonRPC,

    /// Mining-related configuration
    #[partial_struct(ty = "PartialMining")]
    #[partial_struct(serde(default))]
    pub mining: Mining,

    /// Wallet-related configuration
    #[partial_struct(ty = "PartialWallet")]
    #[partial_struct(serde(default))]
    pub wallet: Wallet,

    /// Rocksdb-related configuration
    #[partial_struct(ty = "PartialRocksdb")]
    #[partial_struct(serde(default))]
    pub rocksdb: Rocksdb,

    /// Log-related configuration
    #[partial_struct(ty = "PartialLog")]
    #[partial_struct(serde(default))]
    pub log: Log,
}

/// Log-specific configuration.
#[derive(PartialStruct, Debug, Clone, PartialEq)]
#[partial_struct(derive(Deserialize, Default, Debug, Clone, PartialEq))]
pub struct Log {
    /// Level  for the log messages.
    #[partial_struct(serde(deserialize_with = "as_log_filter"))]
    pub level: log::LevelFilter,
}

fn as_log_filter<'de, D>(deserializer: D) -> Result<Option<log::LevelFilter>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let level_string = String::deserialize(deserializer)?;
    let level = match level_string.as_ref() {
        "off" => log::LevelFilter::Off,
        "error" => log::LevelFilter::Error,
        "warn" => log::LevelFilter::Warn,
        "debug" => log::LevelFilter::Debug,
        "trace" => log::LevelFilter::Trace,
        _ => log::LevelFilter::Info,
    };

    Ok(Some(level))
}

/// Connection-specific configuration.
#[derive(PartialStruct, Debug, Clone, PartialEq)]
#[partial_struct(derive(Deserialize, Default, Debug, Clone, PartialEq))]
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
    #[partial_struct(skip)]
    #[partial_struct(serde(default))]
    pub known_peers: HashSet<SocketAddr>,

    /// Period of the bootstrap peers task
    #[partial_struct(serde(
        default,
        deserialize_with = "from_secs",
        rename = "bootstrap_peers_period_seconds"
    ))]
    pub bootstrap_peers_period: Duration,

    /// Period of the persist peers task
    #[partial_struct(serde(
        default,
        deserialize_with = "from_secs",
        rename = "storage_peers_period_seconds"
    ))]
    pub storage_peers_period: Duration,

    /// Period of the peers discovery task
    #[partial_struct(serde(
        default,
        deserialize_with = "from_secs",
        rename = "discovery_peers_period_seconds"
    ))]
    pub discovery_peers_period: Duration,

    /// Handshake timeout
    #[partial_struct(serde(
        default,
        deserialize_with = "from_secs",
        rename = "handshake_timeout_seconds"
    ))]
    pub handshake_timeout: Duration,

    /// Number of seconds before giving up waiting for requested blocks
    pub blocks_timeout: i64,
}

fn from_secs<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match u64::deserialize(deserializer) {
        Ok(secs) => Some(Duration::from_secs(secs)),
        Err(_) => None,
    })
}

/// Available storage backends
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub enum StorageBackend {
    #[serde(rename = "hashmap")]
    HashMap,
    #[serde(rename = "rocksdb")]
    RocksDB,
}

impl Default for StorageBackend {
    fn default() -> Self {
        StorageBackend::RocksDB
    }
}

/// Storage-specific configuration
#[derive(PartialStruct, Debug, Clone, PartialEq)]
#[partial_struct(derive(Deserialize, Default, Debug, Clone, PartialEq))]
pub struct Storage {
    /// Storage backend to use
    #[partial_struct(skip)]
    #[partial_struct(serde(default))]
    pub backend: StorageBackend,
    /// Whether or not the information should be encrypted before
    /// being stored with this password
    #[partial_struct(skip)]
    #[partial_struct(serde(default))]
    #[partial_struct(serde(deserialize_with = "as_protected_string"))]
    pub password: Option<Protected>,
    /// Path to the directory that will contain the database. Used
    /// only if backend is RocksDB.
    pub db_path: PathBuf,
}

fn as_protected_string<'de, D>(deserializer: D) -> Result<Option<Protected>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let passwd = String::deserialize(deserializer)?;
    Ok(Some(passwd.into()))
}

/// JsonRPC API configuration
#[derive(PartialStruct, Debug, Clone, PartialEq)]
#[partial_struct(derive(Deserialize, Default, Debug, Clone, PartialEq))]
pub struct JsonRPC {
    /// Binary flag telling whether to enable the JSON-RPC interface or not
    pub enabled: bool,
    /// JSON-RPC server address, that is, the socket address (interface ip and
    /// port) for the JSON-RPC server
    pub server_address: SocketAddr,
}

/// Mining-related configuration
#[derive(PartialStruct, Debug, Clone, PartialEq)]
#[partial_struct(derive(Deserialize, Default, Debug, Clone, PartialEq))]
pub struct Mining {
    /// Binary flag telling whether to enable the MiningManager or not
    pub enabled: bool,
}

impl Config {
    pub fn from_partial(config: &PartialConfig) -> Self {
        let defaults: &dyn Defaults = match config.environment {
            Environment::Mainnet => {
                panic!("Config with mainnet environment is currently not allowed");
            }
            Environment::Testnet1 => &Testnet1,
            Environment::Testnet3 => &Testnet3,
        };

        let consensus_constants = match config.environment {
            // When in mainnet, ignore the [consensus_constants] section of the configuration
            Environment::Mainnet => {
                let consensus_constants_no_changes = PartialConsensusConstants::default();
                // Warn the user if the config file contains a non-empty [consensus_constant] section
                if config.consensus_constants != consensus_constants_no_changes {
                    warn!(
                        "Consensus constants in the configuration are ignored when running mainnet"
                    );
                }
                consensus_constants_from_partial(&consensus_constants_no_changes, defaults)
            }
            // In testnet, allow to override the consensus constants
            Environment::Testnet1 => {
                consensus_constants_from_partial(&config.consensus_constants, defaults)
            }
            Environment::Testnet3 => {
                consensus_constants_from_partial(&config.consensus_constants, defaults)
            }
        };

        Config {
            environment: config.environment.clone(),
            connections: Connections::from_partial(&config.connections, defaults),
            storage: Storage::from_partial(&config.storage, defaults),
            log: Log::from_partial(&config.log, defaults),
            consensus_constants,
            jsonrpc: JsonRPC::from_partial(&config.jsonrpc, defaults),
            mining: Mining::from_partial(&config.mining, defaults),
            wallet: Wallet::from_partial(&config.wallet, defaults),
            rocksdb: Rocksdb::from_partial(&config.rocksdb, defaults),
        }
    }
}

pub fn consensus_constants_from_partial(
    config: &PartialConsensusConstants,
    defaults: &dyn Defaults,
) -> ConsensusConstants {
    ConsensusConstants {
        checkpoint_zero_timestamp: config
            .checkpoint_zero_timestamp
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_checkpoint_zero_timestamp()),
        checkpoints_period: config
            .checkpoints_period
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_checkpoints_period()),
        genesis_hash: config
            .genesis_hash
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_genesis_hash()),
        max_block_weight: config
            .max_block_weight
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_max_block_weight()),
        activity_period: config
            .activity_period
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_activity_period()),
        reputation_expire_alpha_diff: config
            .reputation_expire_alpha_diff
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_reputation_expire_alpha_diff()),
        reputation_issuance: config
            .reputation_issuance
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_reputation_issuance()),
        reputation_issuance_stop: config
            .reputation_issuance_stop
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_reputation_issuance_stop()),
        reputation_penalization_factor: config
            .reputation_penalization_factor
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_reputation_penalization_factor()),
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::from_partial(&PartialConfig::default())
    }
}

impl Log {
    pub fn from_partial(config: &PartialLog, defaults: &dyn Defaults) -> Self {
        Log {
            level: config
                .level
                .to_owned()
                .unwrap_or_else(|| defaults.log_level()),
        }
    }
}

impl Connections {
    pub fn from_partial(config: &PartialConnections, defaults: &dyn Defaults) -> Self {
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
            blocks_timeout: config
                .blocks_timeout
                .to_owned()
                .unwrap_or_else(|| defaults.connections_blocks_timeout()),
        }
    }
}

impl Storage {
    pub fn from_partial(config: &PartialStorage, defaults: &dyn Defaults) -> Self {
        Storage {
            backend: config.backend.clone(),
            password: config.password.clone(),
            db_path: config
                .db_path
                .to_owned()
                .unwrap_or_else(|| defaults.storage_db_path()),
        }
    }
}

impl JsonRPC {
    pub fn from_partial(config: &PartialJsonRPC, defaults: &dyn Defaults) -> Self {
        JsonRPC {
            enabled: config
                .enabled
                .to_owned()
                .unwrap_or_else(|| defaults.jsonrpc_enabled()),
            server_address: config
                .server_address
                .to_owned()
                .unwrap_or_else(|| defaults.jsonrpc_server_address()),
        }
    }
}

impl Mining {
    pub fn from_partial(config: &PartialMining, defaults: &dyn Defaults) -> Self {
        Mining {
            enabled: config
                .enabled
                .to_owned()
                .unwrap_or_else(|| defaults.mining_enabled()),
        }
    }
}

/// Wallet-specific configuration.
#[derive(PartialStruct, Serialize, Debug, Clone, PartialEq)]
#[partial_struct(derive(Deserialize, Default, Debug, Clone, PartialEq))]
pub struct Wallet {
    /// Websockets server address.
    pub server_addr: SocketAddr,
    /// Witnet node server address.
    #[partial_struct(skip)]
    #[partial_struct(serde(default))]
    pub node_url: Option<String>,
    /// Database path.
    pub db_path: PathBuf,
    /// Database file name.
    pub db_file_name: String,
    /// Database hash iterations when encrypting.
    pub db_encrypt_hash_iterations: u32,
    /// Database init-vector-length when encrypting.
    pub db_encrypt_iv_length: usize,
    /// Database random salt length when encrypting.
    pub db_encrypt_salt_length: usize,
    /// Master Key-generation seed password. Default empty `""`.
    pub seed_password: ProtectedString,
    /// Master Key-generation salt. Default `Bitcoin seed`.
    pub master_key_salt: Vec<u8>,
    /// Master Key-generation hash iterations. Default `4096`.
    pub id_hash_iterations: u32,
    /// Master Key-generation hash function. Default `Sha256`.
    pub id_hash_function: HashFunction,
    /// Lifetime in seconds of an unlocked wallet session id.
    pub session_expires_in: u64,
    /// Duration in milliseconds after which outgoing request should timeout.
    pub requests_timeout: u64,
}

impl Wallet {
    pub fn from_partial(config: &PartialWallet, defaults: &dyn Defaults) -> Self {
        Wallet {
            session_expires_in: config.session_expires_in.unwrap_or_else(|| 3200),
            requests_timeout: config.requests_timeout.unwrap_or_else(|| 60_000),
            server_addr: config
                .server_addr
                .unwrap_or_else(|| defaults.wallet_server_addr()),
            node_url: config.node_url.clone(),
            db_path: config.db_path.clone().unwrap_or_else(dirs::data_dir),
            db_file_name: config
                .db_file_name
                .clone()
                .unwrap_or_else(|| defaults.wallet_db_file_name()),
            db_encrypt_hash_iterations: config
                .db_encrypt_hash_iterations
                .unwrap_or_else(|| defaults.wallet_db_encrypt_hash_iterations()),
            db_encrypt_iv_length: config
                .db_encrypt_iv_length
                .unwrap_or_else(|| defaults.wallet_db_encrypt_iv_length()),
            db_encrypt_salt_length: config
                .db_encrypt_salt_length
                .unwrap_or_else(|| defaults.wallet_db_encrypt_salt_length()),
            seed_password: config
                .seed_password
                .clone()
                .unwrap_or_else(|| defaults.wallet_seed_password()),
            master_key_salt: config
                .master_key_salt
                .clone()
                .unwrap_or_else(|| defaults.wallet_master_key_salt()),
            id_hash_iterations: config
                .id_hash_iterations
                .unwrap_or_else(|| defaults.wallet_id_hash_iterations()),
            id_hash_function: config
                .id_hash_function
                .clone()
                .unwrap_or_else(|| defaults.wallet_id_hash_function()),
        }
    }
}

/// Rocksdb-specific configuration
#[derive(PartialStruct, Serialize, Debug, Clone, PartialEq)]
#[partial_struct(derive(Deserialize, Default, Debug, Clone, PartialEq))]
pub struct Rocksdb {
    /// By default, RocksDB uses only one background thread for flush and compaction. Calling this
    /// function will set it up such that total of total_threads is used. Good value for
    /// total_threads is the number of cores. You almost definitely want to call this function if
    /// your system is bottlenecked by RocksDB.
    #[partial_struct(skip)]
    increase_parallelism: Option<i32>,
    /// If true, the database will be created if it is missing.
    create_if_missing: bool,
    /// If non-zero, we perform bigger reads when doing compaction. If you're running RocksDB on
    /// spinning disks, you should set this to at least 2MB. That way RocksDB's compaction is doing
    /// sequential instead of random reads.
    compaction_readahead_size: usize,
    /// If true, then every store to stable storage will issue a fsync. If false, then every store
    /// to stable storage will issue a fdatasync. This parameter should be set to true while storing
    /// data to filesystem like ext3 that can lose files after a reboot.
    use_fsync: bool,
    enable_statistics: bool,
}

impl Rocksdb {
    pub fn from_partial(config: &PartialRocksdb, defaults: &dyn Defaults) -> Self {
        Rocksdb {
            increase_parallelism: config.increase_parallelism,
            create_if_missing: config
                .create_if_missing
                .unwrap_or_else(|| defaults.rocksdb_create_if_missing()),
            compaction_readahead_size: config
                .compaction_readahead_size
                .unwrap_or_else(|| defaults.rocksdb_compaction_readahead_size()),
            use_fsync: config
                .use_fsync
                .unwrap_or_else(|| defaults.rocksdb_use_fsync()),
            enable_statistics: config
                .enable_statistics
                .unwrap_or_else(|| defaults.rocksdb_enable_statistics()),
        }
    }

    #[cfg(feature = "rocksdb")]
    pub fn to_rocksdb_options(&self) -> rocksdb::Options {
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(self.create_if_missing);
        opts.set_compaction_readahead_size(self.compaction_readahead_size);
        opts.set_use_fsync(self.use_fsync);

        if let Some(parallelism) = self.increase_parallelism {
            opts.increase_parallelism(parallelism);
        }

        if self.enable_statistics {
            opts.enable_statistics();
        }

        opts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_default_from_partial() {
        let partial_config = PartialStorage::default();
        let config = Storage::from_partial(&partial_config, &Testnet1);

        assert_eq!(config.db_path.to_str(), Testnet1.storage_db_path().to_str());
    }

    #[test]
    fn test_storage_from_partial() {
        let partial_config = PartialStorage {
            backend: StorageBackend::RocksDB,
            password: None,
            db_path: Some(PathBuf::from("other")),
        };
        let config = Storage::from_partial(&partial_config, &Testnet1);

        assert_eq!(config.db_path.to_str(), Some("other"));
    }

    #[test]
    fn test_connections_default_from_partial() {
        let partial_config = PartialConnections::default();
        let config = Connections::from_partial(&partial_config, &Testnet1);

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
        assert_eq!(config.blocks_timeout, Testnet1.connections_blocks_timeout());
    }

    #[test]
    fn test_connections_from_partial() {
        let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();
        let partial_config = PartialConnections {
            server_addr: Some(addr),
            inbound_limit: Some(3),
            outbound_limit: Some(4),
            known_peers: [addr].iter().cloned().collect(),
            bootstrap_peers_period: Some(Duration::from_secs(10)),
            storage_peers_period: Some(Duration::from_secs(60)),
            discovery_peers_period: Some(Duration::from_secs(100)),
            handshake_timeout: Some(Duration::from_secs(3)),
            blocks_timeout: Some(5),
        };
        let config = Connections::from_partial(&partial_config, &Testnet1);

        assert_eq!(config.server_addr, addr);
        assert_eq!(config.inbound_limit, 3);
        assert_eq!(config.outbound_limit, 4);
        assert!(config.known_peers.contains(&addr));
        assert_eq!(config.bootstrap_peers_period, Duration::from_secs(10));
        assert_eq!(config.storage_peers_period, Duration::from_secs(60));
        assert_eq!(config.discovery_peers_period, Duration::from_secs(100));
        assert_eq!(config.handshake_timeout, Duration::from_secs(3));
        assert_eq!(config.blocks_timeout, 5);
    }

    #[test]
    fn test_jsonrpc_default_from_partial() {
        let partial_config = PartialJsonRPC::default();
        let config = JsonRPC::from_partial(&partial_config, &Testnet1);

        assert_eq!(config.server_address, Testnet1.jsonrpc_server_address());
    }

    #[test]
    fn test_jsonrpc_from_partial() {
        let addr: SocketAddr = "127.0.0.1:4000".parse().unwrap();
        let partial_config = PartialJsonRPC {
            enabled: None,
            server_address: Some(addr),
        };
        let config = JsonRPC::from_partial(&partial_config, &Testnet1);

        assert_eq!(config.server_address, addr);
    }

    #[test]
    fn test_config_default_from_partial() {
        let partial_config = PartialConfig::default();
        let config = Config::from_partial(&partial_config);

        assert_eq!(config.environment, Environment::Testnet3);
        assert_eq!(
            config.connections.server_addr,
            Testnet3.connections_server_addr()
        );
        assert_eq!(
            config.connections.inbound_limit,
            Testnet3.connections_inbound_limit()
        );
        assert_eq!(
            config.connections.outbound_limit,
            Testnet3.connections_outbound_limit()
        );
        assert_eq!(
            config.connections.known_peers,
            Testnet3.connections_known_peers()
        );
        assert_eq!(
            config.connections.bootstrap_peers_period,
            Testnet3.connections_bootstrap_peers_period()
        );
        assert_eq!(
            config.connections.storage_peers_period,
            Testnet3.connections_storage_peers_period()
        );
        assert_eq!(
            config.connections.discovery_peers_period,
            Testnet3.connections_discovery_peers_period()
        );
        assert_eq!(
            config.connections.handshake_timeout,
            Testnet3.connections_handshake_timeout()
        );
        assert_eq!(config.storage.db_path, Testnet3.storage_db_path());
        assert_eq!(
            config.jsonrpc.server_address,
            Testnet3.jsonrpc_server_address()
        );
        assert_eq!(
            config.connections.blocks_timeout,
            Testnet3.connections_blocks_timeout()
        );
    }
}
