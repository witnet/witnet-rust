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
//! Config { environment: Environment::Testnet, ... }
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
use std::{
    collections::HashSet, fmt, marker::PhantomData, net::SocketAddr, path::PathBuf, time::Duration,
};

use serde::{de, Deserialize, Deserializer, Serialize};

use crate::{
    defaults::{Defaults, Development, Mainnet, Testnet},
    dirs,
};
use partial_struct::PartialStruct;
use std::convert::TryFrom;
use witnet_crypto::hash::HashFunction;
use witnet_data_structures::chain::{ConsensusConstants, Environment, PartialConsensusConstants};
use witnet_protected::ProtectedString;

/// The total configuration object that contains all other, more
/// specific, configuration objects (connections, storage, etc).
#[derive(PartialStruct, Debug, Clone, PartialEq)]
#[partial_struct(derive(Deserialize, Serialize, Default, Debug, Clone, PartialEq))]
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

    /// Ntp-related configuration
    #[partial_struct(ty = "PartialNtp")]
    #[partial_struct(serde(default))]
    pub ntp: Ntp,

    /// Mempool-related configuration
    #[partial_struct(ty = "PartialMempool")]
    #[partial_struct(serde(default))]
    pub mempool: Mempool,

    /// Threshold Activation of Protocol Improvements
    #[partial_struct(skip)]
    #[partial_struct(serde(default))]
    pub tapi: Tapi,
}

/// Log-specific configuration.
#[derive(PartialStruct, Debug, Clone, PartialEq)]
#[partial_struct(derive(Deserialize, Serialize, Default, Debug, Clone, PartialEq))]
pub struct Log {
    /// Level for the log messages.
    #[partial_struct(serde(
        deserialize_with = "as_log_filter",
        serialize_with = "as_log_filter_string"
    ))]
    pub level: log::LevelFilter,
    /// Automated bug reporting (helps the community improve the software)
    pub sentry_telemetry: bool,
}

/// Connection-specific configuration.
#[derive(PartialStruct, Debug, Clone, PartialEq)]
#[partial_struct(derive(Deserialize, Serialize, Default, Debug, Clone, PartialEq))]
pub struct Connections {
    /// Server address, that is, the socket address (interface ip and
    /// port) to which the server accepting connections from other
    /// peers should bind to
    pub server_addr: SocketAddr,

    /// Public address
    #[partial_struct(skip)]
    #[partial_struct(serde(default))]
    pub public_addr: Option<SocketAddr>,

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
        serialize_with = "to_secs",
        deserialize_with = "from_secs",
        rename = "bootstrap_peers_period_seconds"
    ))]
    pub bootstrap_peers_period: Duration,

    /// Period of the persist peers task
    #[partial_struct(serde(
        default,
        serialize_with = "to_secs",
        deserialize_with = "from_secs",
        rename = "storage_peers_period_seconds"
    ))]
    pub storage_peers_period: Duration,

    /// Period of the peers discovery task
    #[partial_struct(serde(
        default,
        serialize_with = "to_secs",
        deserialize_with = "from_secs",
        rename = "discovery_peers_period_seconds"
    ))]
    pub discovery_peers_period: Duration,

    /// Period of the peers melt task
    #[partial_struct(serde(
        default,
        serialize_with = "to_secs",
        deserialize_with = "from_secs",
        rename = "check_melted_peers_period_seconds"
    ))]
    pub check_melted_peers_period: Duration,

    /// Period of the feeler task (try_peer)
    #[partial_struct(serde(
        default,
        serialize_with = "to_secs",
        deserialize_with = "from_secs",
        rename = "feeler_peers_period_seconds"
    ))]
    pub feeler_peers_period: Duration,

    /// Handshake timeout
    #[partial_struct(serde(
        default,
        serialize_with = "to_secs",
        deserialize_with = "from_secs",
        rename = "handshake_timeout_seconds"
    ))]
    pub handshake_timeout: Duration,

    /// Handshake maximum timestamp difference in seconds
    /// Set to 0 to disable timestamp comparison in handshake
    pub handshake_max_ts_diff: i64,

    /// Number of seconds before giving up waiting for requested blocks
    pub blocks_timeout: i64,

    /// Constant to specify when consensus is achieved (in %)
    pub consensus_c: u32,

    /// Period that indicate the validity of a checked peer
    pub bucketing_update_period: i64,

    /// Period in seconds for a potential peer address to be kept "iced", i.e. will not be tried
    /// again before that amount of time.
    #[partial_struct(serde(
        default,
        serialize_with = "to_secs",
        deserialize_with = "from_secs",
        rename = "bucketing_ice_period_seconds"
    ))]
    pub bucketing_ice_period: Duration,

    /// Reject (tarpit) inbound connections coming from addresses that are alike, so as
    /// to prevent sybil peers from monopolizing our inbound capacity.
    pub reject_sybil_inbounds: bool,

    /// Limit to reject (tarpit) inbound connections. If the limit is set to 18, the addresses having
    /// the same first 18 bits in the IP will collide, so as to prevent sybil peers from monopolizing our inbound capacity.
    pub reject_sybil_inbounds_range_limit: u8,

    /// Limit the number of requested blocks that will be processed as one batch
    pub requested_blocks_batch_limit: u32,
}

/// Available storage backends
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
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
#[partial_struct(derive(Deserialize, Serialize, Default, Debug, Clone, PartialEq))]
pub struct Storage {
    /// Storage backend to use
    #[partial_struct(skip)]
    #[partial_struct(serde(default))]
    pub backend: StorageBackend,
    /// Path to the directory that will contain the database. Used
    /// only if backend is RocksDB.
    pub db_path: PathBuf,
    /// Path to the master key to import when initializing the node
    #[partial_struct(skip)]
    #[partial_struct(serde(default))]
    pub master_key_import_path: Option<PathBuf>,
}

/// JsonRPC API configuration
#[derive(PartialStruct, Debug, Clone, PartialEq)]
#[partial_struct(derive(Deserialize, Serialize, Default, Debug, Clone, PartialEq))]
pub struct JsonRPC {
    /// Binary flag telling whether to enable the JSON-RPC interface or not
    pub enabled: bool,
    /// JSON-RPC server address, that is, the socket address (interface ip and
    /// port) for the JSON-RPC server
    pub server_address: SocketAddr,
    /// Enable methods not suitable for shared nodes
    pub enable_sensitive_methods: bool,
}

/// Mining-related configuration
#[derive(PartialStruct, Debug, Clone, PartialEq)]
#[partial_struct(derive(Deserialize, Serialize, Default, Debug, Clone, PartialEq))]
pub struct Mining {
    /// Binary flag telling whether to enable the MiningManager or not
    pub enabled: bool,
    /// Limits the number of retrievals to perform during a single epoch.
    /// This tries to prevent nodes from forking out or being left in a
    /// bad condition if hitting bandwidth or CPU bottlenecks.
    /// Set to 0 totally disable participation in resolving data requests.
    pub data_request_max_retrievals_per_epoch: u16,
    /// Timeout for data request retrieval and aggregation execution.
    /// This should usually be slightly below half the checkpoints period.
    /// Set to 0 to disable timeouts.
    #[partial_struct(serde(
        default,
        deserialize_with = "from_millis",
        serialize_with = "to_millis",
        rename = "data_request_timeout_milliseconds"
    ))]
    pub data_request_timeout: Duration,
    /// Genesis block path
    pub genesis_path: String,
    /// Percentage to redistribute mint reward in another address
    pub mint_external_percentage: u8,
    /// Address where redistribute mint reward
    #[partial_struct(skip)]
    #[partial_struct(serde(default))]
    pub mint_external_address: Option<String>,
    /// Mempool size limit in weight units
    pub transactions_pool_total_weight_limit: u64,
    /// Minimum value transfer transaction fee that allows being included into a block
    #[partial_struct(serde(default, rename = "minimum_vtt_fee_nanowits"))]
    pub minimum_vtt_fee: u64,
}

/// NTP-related configuration
#[derive(PartialStruct, Debug, Clone, PartialEq)]
#[partial_struct(derive(Deserialize, Serialize, Default, Debug, Clone, PartialEq))]
pub struct Ntp {
    /// Period that indicate the validity of a ntp timestamp
    #[partial_struct(serde(
        default,
        serialize_with = "to_secs",
        deserialize_with = "from_secs",
        rename = "update_period_seconds"
    ))]
    pub update_period: Duration,

    /// Server to query ntp information
    pub servers: Vec<String>,

    /// Enable NTP for clock synchronization
    pub enabled: bool,
}

/// Mempool-related configuration
#[derive(PartialStruct, Debug, Clone, PartialEq)]
#[partial_struct(derive(Deserialize, Serialize, Default, Debug, Clone, PartialEq))]
pub struct Mempool {
    /// Timeout to use again an UTXO spent by a pending transaction
    pub tx_pending_timeout: u64,
    /// Maximum number of recovered transactions to include by epoch
    pub max_reinserted_transactions: u32,
}

/// Threshold Activation of Protocol Improvements
///
/// Allow miners to oppose activation of future protocol improvements even if their nodes
/// do implement the required logic.
#[derive(Deserialize, Serialize, Default, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct Tapi {
    /// Oppose WIP0020
    pub oppose_wip0020: bool,
    /// Oppose WIP0021
    pub oppose_wip0021: bool,
}

fn to_partial_consensus_constants(c: &ConsensusConstants) -> PartialConsensusConstants {
    PartialConsensusConstants {
        checkpoint_zero_timestamp: Some(c.checkpoint_zero_timestamp),
        checkpoints_period: Some(c.checkpoints_period),
        bootstrap_hash: Some(c.bootstrap_hash),
        genesis_hash: Some(c.genesis_hash),
        max_vt_weight: Some(c.max_vt_weight),
        max_dr_weight: Some(c.max_dr_weight),
        activity_period: Some(c.activity_period),
        reputation_expire_alpha_diff: Some(c.reputation_expire_alpha_diff),
        reputation_issuance: Some(c.reputation_issuance),
        reputation_issuance_stop: Some(c.reputation_issuance_stop),
        reputation_penalization_factor: Some(c.reputation_penalization_factor),
        mining_backup_factor: Some(c.mining_backup_factor),
        mining_replication_factor: Some(c.mining_replication_factor),
        collateral_minimum: Some(c.collateral_minimum),
        collateral_age: Some(c.collateral_age),
        superblock_period: Some(c.superblock_period),
        extra_rounds: Some(c.extra_rounds),
        minimum_difficulty: Some(c.minimum_difficulty),
        epochs_with_minimum_difficulty: Some(c.epochs_with_minimum_difficulty),
        bootstrapping_committee: Some(c.bootstrapping_committee.clone()),
        superblock_signing_committee_size: Some(c.superblock_signing_committee_size),
        superblock_committee_decreasing_period: Some(c.superblock_committee_decreasing_period),
        superblock_committee_decreasing_step: Some(c.superblock_committee_decreasing_step),
        initial_block_reward: Some(c.initial_block_reward),
        halving_period: Some(c.halving_period),
    }
}

impl Config {
    pub fn from_partial(config: &PartialConfig) -> Self {
        let defaults: &dyn Defaults = match config.environment {
            Environment::Development => &Development,
            Environment::Mainnet => &Mainnet,
            Environment::Testnet => &Testnet,
        };

        let consensus_constants = if config.environment.can_override_consensus_constants() {
            consensus_constants_from_partial(&config.consensus_constants, defaults)
        } else {
            consensus_constants_from_partial(&PartialConsensusConstants::default(), defaults)
        };

        Config {
            environment: config.environment,
            connections: Connections::from_partial(&config.connections, defaults),
            storage: Storage::from_partial(&config.storage, defaults),
            log: Log::from_partial(&config.log, defaults),
            consensus_constants,
            jsonrpc: JsonRPC::from_partial(&config.jsonrpc, defaults),
            mining: Mining::from_partial(&config.mining, defaults),
            wallet: Wallet::from_partial(&config.wallet, defaults),
            rocksdb: Rocksdb::from_partial(&config.rocksdb, defaults),
            ntp: Ntp::from_partial(&config.ntp, defaults),
            mempool: Mempool::from_partial(&config.mempool, defaults),
            tapi: config.tapi.clone(),
        }
    }

    pub fn to_partial(&self) -> PartialConfig {
        PartialConfig {
            environment: self.environment,
            connections: self.connections.to_partial(),
            storage: self.storage.to_partial(),
            log: self.log.to_partial(),
            consensus_constants: to_partial_consensus_constants(&self.consensus_constants),
            jsonrpc: self.jsonrpc.to_partial(),
            mining: self.mining.to_partial(),
            wallet: self.wallet.to_partial(),
            rocksdb: self.rocksdb.to_partial(),
            ntp: self.ntp.to_partial(),
            mempool: self.mempool.to_partial(),
            tapi: self.tapi.clone(),
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
        superblock_period: config
            .superblock_period
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_superblock_period()),
        bootstrap_hash: config
            .bootstrap_hash
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_bootstrap_hash()),
        genesis_hash: config
            .genesis_hash
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_genesis_hash()),
        max_vt_weight: config
            .max_vt_weight
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_max_vt_weight()),
        max_dr_weight: config
            .max_dr_weight
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_max_dr_weight()),
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
        minimum_difficulty: config
            .minimum_difficulty
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_minimum_difficulty()),
        epochs_with_minimum_difficulty: config
            .epochs_with_minimum_difficulty
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_epochs_with_minimum_difficulty()),
        reputation_issuance_stop: config
            .reputation_issuance_stop
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_reputation_issuance_stop()),
        reputation_penalization_factor: config
            .reputation_penalization_factor
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_reputation_penalization_factor()),
        mining_backup_factor: config
            .mining_backup_factor
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_mining_backup_factor()),
        mining_replication_factor: config
            .mining_replication_factor
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_mining_replication_factor()),
        collateral_minimum: config
            .collateral_minimum
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_collateral_minimum()),
        bootstrapping_committee: config
            .bootstrapping_committee
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_bootstrapping_committee()),
        collateral_age: config
            .collateral_age
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_collateral_age()),
        extra_rounds: config
            .extra_rounds
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_extra_rounds()),
        superblock_signing_committee_size: config
            .superblock_signing_committee_size
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_superblock_signing_committee_size()),
        superblock_committee_decreasing_period: config
            .superblock_committee_decreasing_period
            .to_owned()
            .unwrap_or_else(|| {
                defaults.consensus_constants_superblock_committee_decreasing_period()
            }),
        superblock_committee_decreasing_step: config
            .superblock_committee_decreasing_step
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_superblock_committee_decreasing_step()),
        initial_block_reward: config
            .initial_block_reward
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_initial_block_reward()),
        halving_period: config
            .halving_period
            .to_owned()
            .unwrap_or_else(|| defaults.consensus_constants_halving_period()),
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
            sentry_telemetry: config.sentry_telemetry.unwrap_or(false),
        }
    }

    pub fn to_partial(&self) -> PartialLog {
        PartialLog {
            level: Some(self.level),
            sentry_telemetry: Some(self.sentry_telemetry),
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
            public_addr: config.public_addr,
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
            check_melted_peers_period: config
                .check_melted_peers_period
                .to_owned()
                .unwrap_or_else(|| defaults.connections_check_melted_peers_period()),
            feeler_peers_period: config
                .feeler_peers_period
                .to_owned()
                .unwrap_or_else(|| defaults.connections_feeler_peers_period()),
            handshake_timeout: config
                .handshake_timeout
                .unwrap_or_else(|| defaults.connections_handshake_timeout()),
            handshake_max_ts_diff: config
                .handshake_max_ts_diff
                .to_owned()
                .unwrap_or_else(|| defaults.connections_handshake_max_ts_diff()),
            blocks_timeout: config
                .blocks_timeout
                .to_owned()
                .unwrap_or_else(|| defaults.connections_blocks_timeout()),
            consensus_c: config
                .consensus_c
                .to_owned()
                .unwrap_or_else(|| defaults.connections_consensus_c()),
            bucketing_ice_period: config
                .bucketing_ice_period
                .to_owned()
                .unwrap_or_else(|| defaults.connections_bucketing_ice_period()),
            bucketing_update_period: config
                .bucketing_update_period
                .to_owned()
                .unwrap_or_else(|| defaults.connections_bucketing_update_period()),
            reject_sybil_inbounds: config
                .reject_sybil_inbounds
                .to_owned()
                .unwrap_or_else(|| defaults.connections_reject_sybil_inbounds()),
            reject_sybil_inbounds_range_limit: config
                .reject_sybil_inbounds_range_limit
                .to_owned()
                .unwrap_or_else(|| defaults.connections_reject_sybil_inbounds_range_limit()),
            requested_blocks_batch_limit: config
                .requested_blocks_batch_limit
                .to_owned()
                .unwrap_or_else(|| defaults.connections_requested_blocks_batch_limit()),
        }
    }

    pub fn to_partial(&self) -> PartialConnections {
        PartialConnections {
            server_addr: Some(self.server_addr),
            public_addr: self.public_addr,
            inbound_limit: Some(self.inbound_limit),
            outbound_limit: Some(self.outbound_limit),
            known_peers: self.known_peers.clone(),
            bootstrap_peers_period: Some(self.bootstrap_peers_period),
            storage_peers_period: Some(self.storage_peers_period),
            discovery_peers_period: Some(self.discovery_peers_period),
            check_melted_peers_period: Some(self.check_melted_peers_period),
            feeler_peers_period: Some(self.feeler_peers_period),
            handshake_timeout: Some(self.handshake_timeout),
            handshake_max_ts_diff: Some(self.handshake_max_ts_diff),
            blocks_timeout: Some(self.blocks_timeout),
            consensus_c: Some(self.consensus_c),
            bucketing_ice_period: Some(self.bucketing_ice_period),
            bucketing_update_period: Some(self.bucketing_update_period),
            reject_sybil_inbounds: Some(self.reject_sybil_inbounds),
            reject_sybil_inbounds_range_limit: Some(self.reject_sybil_inbounds_range_limit),
            requested_blocks_batch_limit: Some(self.requested_blocks_batch_limit),
        }
    }
}

impl Storage {
    pub fn from_partial(config: &PartialStorage, defaults: &dyn Defaults) -> Self {
        Storage {
            backend: config.backend.clone(),
            db_path: config
                .db_path
                .to_owned()
                .unwrap_or_else(|| defaults.storage_db_path()),
            master_key_import_path: config.master_key_import_path.clone(),
        }
    }

    pub fn to_partial(&self) -> PartialStorage {
        PartialStorage {
            backend: self.backend.clone(),
            db_path: Some(self.db_path.clone()),
            master_key_import_path: self.master_key_import_path.clone(),
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
            enable_sensitive_methods: config
                .enable_sensitive_methods
                .to_owned()
                .unwrap_or_else(|| defaults.jsonrpc_enable_sensitive_methods()),
        }
    }

    pub fn to_partial(&self) -> PartialJsonRPC {
        PartialJsonRPC {
            enabled: Some(self.enabled),
            server_address: Some(self.server_address),
            enable_sensitive_methods: Some(self.enable_sensitive_methods),
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
            data_request_timeout: config
                .data_request_timeout
                .to_owned()
                .unwrap_or_else(|| defaults.mining_data_request_timeout()),
            data_request_max_retrievals_per_epoch: config
                .data_request_max_retrievals_per_epoch
                .to_owned()
                .unwrap_or_else(|| defaults.mining_data_request_max_retrievals_per_epoch()),
            genesis_path: config
                .genesis_path
                .clone()
                .unwrap_or_else(|| defaults.mining_genesis_path()),
            mint_external_percentage: config
                .mint_external_percentage
                .to_owned()
                .unwrap_or_else(|| defaults.mining_mint_external_percentage()),
            mint_external_address: config.mint_external_address.clone(),
            transactions_pool_total_weight_limit: config
                .transactions_pool_total_weight_limit
                .to_owned()
                .unwrap_or_else(|| defaults.mining_transactions_pool_total_weight_limit()),
            minimum_vtt_fee: config
                .minimum_vtt_fee
                .to_owned()
                .unwrap_or_else(|| defaults.mining_minimum_vtt_fee()),
        }
    }

    pub fn to_partial(&self) -> PartialMining {
        PartialMining {
            enabled: Some(self.enabled),
            data_request_timeout: Some(self.data_request_timeout),
            data_request_max_retrievals_per_epoch: Some(self.data_request_max_retrievals_per_epoch),
            genesis_path: Some(self.genesis_path.clone()),
            mint_external_percentage: Some(self.mint_external_percentage),
            mint_external_address: self.mint_external_address.clone(),
            transactions_pool_total_weight_limit: Some(self.transactions_pool_total_weight_limit),
            minimum_vtt_fee: Some(self.minimum_vtt_fee),
        }
    }
}

impl Ntp {
    pub fn from_partial(config: &PartialNtp, defaults: &dyn Defaults) -> Self {
        Ntp {
            update_period: config
                .update_period
                .to_owned()
                .unwrap_or_else(|| defaults.ntp_update_period()),
            servers: config
                .servers
                .clone()
                .unwrap_or_else(|| defaults.ntp_server()),
            enabled: config
                .enabled
                .to_owned()
                .unwrap_or_else(|| defaults.ntp_enabled()),
        }
    }

    pub fn to_partial(&self) -> PartialNtp {
        PartialNtp {
            update_period: Some(self.update_period),
            servers: Some(self.servers.clone()),
            enabled: Some(self.enabled),
        }
    }
}

impl Mempool {
    pub fn from_partial(config: &PartialMempool, defaults: &dyn Defaults) -> Self {
        Mempool {
            tx_pending_timeout: config
                .tx_pending_timeout
                .to_owned()
                .unwrap_or_else(|| defaults.mempool_tx_pending_timeout()),
            max_reinserted_transactions: config
                .max_reinserted_transactions
                .to_owned()
                .unwrap_or_else(|| defaults.mempool_max_reinserted_transactions()),
        }
    }

    pub fn to_partial(&self) -> PartialMempool {
        PartialMempool {
            tx_pending_timeout: Some(self.tx_pending_timeout),
            max_reinserted_transactions: Some(self.max_reinserted_transactions),
        }
    }
}

/// Wallet-specific configuration.
#[derive(PartialStruct, Serialize, Debug, Clone, PartialEq)]
#[partial_struct(derive(Deserialize, Serialize, Default, Debug, Clone, PartialEq))]
pub struct Wallet {
    /// Whether or not this wallet will comunicate with a testnet node.
    #[partial_struct(skip)]
    #[partial_struct(serde(default))]
    pub testnet: bool,
    /// Websockets server address.
    pub server_addr: SocketAddr,
    /// Witnet node server address.
    /// If more than one address is provided, will choose one at random.
    #[partial_struct(skip)]
    #[partial_struct(serde(default))]
    #[partial_struct(serde(deserialize_with = "deserialize_one_or_many"))]
    pub node_url: Vec<String>,
    /// How many blocks to ask a Witnet node for when synchronizing.
    pub node_sync_batch_size: u32,
    /// How many worker threads the wallet uses.
    #[partial_struct(skip)]
    #[partial_struct(serde(default))]
    pub concurrency: Option<usize>,
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
    /// Length of the batch of transient addresses to be used for synchronization purposes
    /// (e.g. for re-importing a wallet with seed phrase).
    pub sync_address_batch_length: u16,
    /// Allow to use outputs that have not been confirmed by a superblock in new transactions
    pub use_unconfirmed_utxos: bool,
}

impl Wallet {
    pub fn from_partial(config: &PartialWallet, defaults: &dyn Defaults) -> Self {
        Wallet {
            testnet: config.testnet,
            session_expires_in: config.session_expires_in.unwrap_or(900),
            requests_timeout: config.requests_timeout.unwrap_or(5_000),
            server_addr: config
                .server_addr
                .unwrap_or_else(|| defaults.wallet_server_addr()),
            node_url: config.node_url.clone(),
            node_sync_batch_size: config.node_sync_batch_size.unwrap_or(50),
            concurrency: config.concurrency,
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
            sync_address_batch_length: config
                .sync_address_batch_length
                .unwrap_or_else(|| defaults.wallet_sync_address_batch_length()),
            use_unconfirmed_utxos: config
                .use_unconfirmed_utxos
                .unwrap_or_else(|| defaults.wallet_use_unconfirmed_utxos()),
        }
    }

    pub fn to_partial(&self) -> PartialWallet {
        PartialWallet {
            testnet: self.testnet,
            server_addr: Some(self.server_addr),
            node_url: self.node_url.clone(),
            node_sync_batch_size: Some(self.node_sync_batch_size),
            concurrency: self.concurrency,
            db_path: Some(self.db_path.clone()),
            db_file_name: Some(self.db_file_name.clone()),
            db_encrypt_hash_iterations: Some(self.db_encrypt_hash_iterations),
            db_encrypt_iv_length: Some(self.db_encrypt_iv_length),
            db_encrypt_salt_length: Some(self.db_encrypt_salt_length),
            seed_password: None,   // seed_password should not be exported
            master_key_salt: None, // master_key_salt should not be exported
            id_hash_iterations: Some(self.id_hash_iterations),
            id_hash_function: Some(self.id_hash_function.clone()),
            session_expires_in: Some(self.session_expires_in),
            requests_timeout: Some(self.requests_timeout),
            sync_address_batch_length: Some(self.sync_address_batch_length),
            use_unconfirmed_utxos: Some(self.use_unconfirmed_utxos),
        }
    }
}

/// Rocksdb-specific configuration
#[derive(PartialStruct, Serialize, Debug, Clone, PartialEq)]
#[partial_struct(derive(Deserialize, Serialize, Default, Debug, Clone, PartialEq))]
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

    pub fn to_partial(&self) -> PartialRocksdb {
        PartialRocksdb {
            increase_parallelism: self.increase_parallelism,
            create_if_missing: Some(self.create_if_missing),
            compaction_readahead_size: Some(self.compaction_readahead_size),
            use_fsync: Some(self.use_fsync),
            enable_statistics: Some(self.enable_statistics),
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

// Serialization helpers

fn as_log_filter_string<S>(
    level: &Option<log::LevelFilter>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if let Some(level_unwrapped) = level {
        let result = match level_unwrapped {
            log::LevelFilter::Off => "off",
            log::LevelFilter::Error => "error",
            log::LevelFilter::Warn => "warn",
            log::LevelFilter::Debug => "debug",
            log::LevelFilter::Trace => "trace",
            _ => "info",
        };
        serializer.serialize_str(result)
    } else {
        serializer.serialize_str("info")
    }
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

fn to_millis<S>(val: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if let Some(duration) = val {
        serializer
            .serialize_u64(u64::try_from(duration.as_millis()).map_err(serde::ser::Error::custom)?)
    } else {
        serializer.serialize_none()
    }
}

#[allow(clippy::unnecessary_wraps)]
fn from_millis<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match u64::deserialize(deserializer) {
        Ok(secs) => Some(Duration::from_millis(secs)),
        Err(_) => None,
    })
}
fn to_secs<S>(val: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if let Some(duration) = val {
        serializer.serialize_u64(duration.as_secs())
    } else {
        serializer.serialize_none()
    }
}

#[allow(clippy::unnecessary_wraps)]
fn from_secs<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match u64::deserialize(deserializer) {
        Ok(secs) => Some(Duration::from_secs(secs)),
        Err(_) => None,
    })
}

// https://stackoverflow.com/a/43627388
fn deserialize_one_or_many<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringOrVec(PhantomData<Vec<String>>);

    impl<'de> de::Visitor<'de> for StringOrVec {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or list of strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(vec![value.to_owned()])
        }

        fn visit_seq<S>(self, visitor: S) -> Result<Self::Value, S::Error>
        where
            S: de::SeqAccess<'de>,
        {
            Deserialize::deserialize(de::value::SeqAccessDeserializer::new(visitor))
        }
    }

    deserializer.deserialize_any(StringOrVec(PhantomData))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_default_from_partial() {
        let partial_config = PartialStorage::default();
        let config = Storage::from_partial(&partial_config, &Testnet);

        assert_eq!(config.db_path.to_str(), Testnet.storage_db_path().to_str());
    }

    #[test]
    fn test_storage_from_partial() {
        let partial_config = PartialStorage {
            backend: StorageBackend::RocksDB,
            db_path: Some(PathBuf::from("other")),
            master_key_import_path: None,
        };
        let config = Storage::from_partial(&partial_config, &Testnet);

        assert_eq!(config.db_path.to_str(), Some("other"));
    }

    #[test]
    fn test_connections_default_from_partial() {
        let partial_config = PartialConnections::default();
        let config = Connections::from_partial(&partial_config, &Testnet);

        assert_eq!(config.server_addr, Testnet.connections_server_addr());
        assert_eq!(config.inbound_limit, Testnet.connections_inbound_limit());
        assert_eq!(config.outbound_limit, Testnet.connections_outbound_limit());
        assert_eq!(config.known_peers, Testnet.connections_known_peers());
        assert_eq!(
            config.bootstrap_peers_period,
            Testnet.connections_bootstrap_peers_period()
        );
        assert_eq!(
            config.storage_peers_period,
            Testnet.connections_storage_peers_period()
        );
        assert_eq!(
            config.discovery_peers_period,
            Testnet.connections_discovery_peers_period()
        );
        assert_eq!(
            config.handshake_timeout,
            Testnet.connections_handshake_timeout()
        );
        assert_eq!(config.blocks_timeout, Testnet.connections_blocks_timeout());
    }

    #[test]
    fn test_connections_from_partial() {
        let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();
        let public_addr: SocketAddr = "127.0.0.1:3003".parse().unwrap();
        let partial_config = PartialConnections {
            server_addr: Some(addr),
            public_addr: Some(public_addr),
            inbound_limit: Some(3),
            outbound_limit: Some(4),
            known_peers: [addr].iter().cloned().collect(),
            bootstrap_peers_period: Some(Duration::from_secs(10)),
            storage_peers_period: Some(Duration::from_secs(60)),
            discovery_peers_period: Some(Duration::from_secs(100)),
            check_melted_peers_period: Some(Duration::from_secs(112)),
            feeler_peers_period: Some(Duration::from_secs(1)),
            handshake_timeout: Some(Duration::from_secs(3)),
            handshake_max_ts_diff: Some(17),
            blocks_timeout: Some(5),
            consensus_c: Some(51),
            bucketing_ice_period: Some(Duration::from_secs(13200)),
            bucketing_update_period: Some(200),
            reject_sybil_inbounds: Some(true),
            reject_sybil_inbounds_range_limit: Some(14),
            requested_blocks_batch_limit: Some(99),
        };
        let config = Connections::from_partial(&partial_config, &Testnet);

        assert_eq!(config.server_addr, addr);
        assert_eq!(config.public_addr, Some(public_addr));
        assert_eq!(config.inbound_limit, 3);
        assert_eq!(config.outbound_limit, 4);
        assert!(config.known_peers.contains(&addr));
        assert_eq!(config.bootstrap_peers_period, Duration::from_secs(10));
        assert_eq!(config.storage_peers_period, Duration::from_secs(60));
        assert_eq!(config.discovery_peers_period, Duration::from_secs(100));
        assert_eq!(config.check_melted_peers_period, Duration::from_secs(112));
        assert_eq!(config.feeler_peers_period, Duration::from_secs(1));
        assert_eq!(config.handshake_timeout, Duration::from_secs(3));
        assert_eq!(config.blocks_timeout, 5);
        assert_eq!(config.handshake_max_ts_diff, 17);
        assert_eq!(config.consensus_c, 51);
        assert_eq!(config.bucketing_ice_period, Duration::from_secs(13200));
        assert_eq!(config.bucketing_update_period, 200);
        assert!(config.reject_sybil_inbounds);
        assert_eq!(config.reject_sybil_inbounds_range_limit, 14);
        assert_eq!(config.requested_blocks_batch_limit, 99);
    }

    #[test]
    fn test_jsonrpc_default_from_partial() {
        let partial_config = PartialJsonRPC::default();
        let config = JsonRPC::from_partial(&partial_config, &Testnet);

        assert_eq!(config.server_address, Testnet.jsonrpc_server_address());
    }

    #[test]
    fn test_jsonrpc_from_partial() {
        let addr: SocketAddr = "127.0.0.1:4000".parse().unwrap();
        let partial_config = PartialJsonRPC {
            enabled: None,
            server_address: Some(addr),
            enable_sensitive_methods: None,
        };
        let config = JsonRPC::from_partial(&partial_config, &Testnet);

        assert_eq!(config.server_address, addr);
    }

    #[test]
    fn test_config_default_from_partial() {
        let partial_config = PartialConfig::default();
        let config = Config::from_partial(&partial_config);

        assert_eq!(config.environment, Environment::Mainnet);
        assert_eq!(
            config.connections.server_addr,
            Mainnet.connections_server_addr()
        );
        assert_eq!(
            config.connections.inbound_limit,
            Mainnet.connections_inbound_limit()
        );
        assert_eq!(
            config.connections.outbound_limit,
            Mainnet.connections_outbound_limit()
        );
        assert_eq!(
            config.connections.known_peers,
            Mainnet.connections_known_peers()
        );
        assert_eq!(
            config.connections.bootstrap_peers_period,
            Mainnet.connections_bootstrap_peers_period()
        );
        assert_eq!(
            config.connections.storage_peers_period,
            Mainnet.connections_storage_peers_period()
        );
        assert_eq!(
            config.connections.discovery_peers_period,
            Mainnet.connections_discovery_peers_period()
        );
        assert_eq!(
            config.connections.feeler_peers_period,
            Mainnet.connections_feeler_peers_period()
        );
        assert_eq!(
            config.connections.handshake_timeout,
            Mainnet.connections_handshake_timeout()
        );
        assert_eq!(config.storage.db_path, Mainnet.storage_db_path());
        assert_eq!(
            config.jsonrpc.server_address,
            Mainnet.jsonrpc_server_address()
        );
        assert_eq!(
            config.connections.blocks_timeout,
            Mainnet.connections_blocks_timeout()
        );
        assert_eq!(
            config.connections.consensus_c,
            Mainnet.connections_consensus_c()
        );
        assert_eq!(
            config.connections.bucketing_update_period,
            Mainnet.connections_bucketing_update_period()
        );
    }
}
