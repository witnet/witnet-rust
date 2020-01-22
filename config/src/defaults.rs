//! # Default per-environment values
//!
//! This module contains per-environment default values for the Witnet
//! protocol params.
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use witnet_crypto::hash::HashFunction;
use witnet_data_structures::chain::Hash;
use witnet_protected::ProtectedString;

// When changing the defaults, remember to update the documentation!
// https://github.com/witnet/witnet-rust/blob/master/docs/configuration/toml-file.md
// https://github.com/witnet/witnet-rust/blob/master/docs/configuration/environment.md

/// Trait defining all the configuration params that have a suitable
/// default value depending on the environment (mainnet, testnet,
/// etc).
pub trait Defaults {
    /// Default log level
    fn log_level(&self) -> log::LevelFilter {
        log::LevelFilter::Info
    }

    /// Default server addr
    fn connections_server_addr(&self) -> SocketAddr;

    /// Default inbound limit for connections: `128`
    fn connections_inbound_limit(&self) -> u16 {
        128
    }

    /// Default outbound limit for connections: `8`
    fn connections_outbound_limit(&self) -> u16 {
        8
    }

    /// Default known peers: none
    fn connections_known_peers(&self) -> HashSet<SocketAddr> {
        HashSet::new()
    }

    /// Default path for the database
    fn storage_db_path(&self) -> PathBuf;

    /// Default period for bootstrap peers
    fn connections_bootstrap_peers_period(&self) -> Duration {
        Duration::from_secs(5)
    }

    /// Default period for persist peers into storage
    fn connections_storage_peers_period(&self) -> Duration {
        Duration::from_secs(30)
    }

    /// Default period for discovering peers
    fn connections_discovery_peers_period(&self) -> Duration {
        Duration::from_secs(30)
    }

    /// Default period for trying connecting to recently discovered peer addresses
    /// in search for quality peers
    fn connections_feeler_peers_period(&self) -> Duration {
        Duration::from_secs(2)
    }

    /// Default handshake timeout
    fn connections_handshake_timeout(&self) -> Duration {
        Duration::from_secs(5)
    }

    /// Constant to specify when consensus is achieved (in %)
    fn connections_consensus_c(&self) -> u32 {
        70
    }

    /// Period that indicate the validity of a checked peer
    fn connections_bucketing_update_period(&self) -> i64 {
        300
    }

    /// Timestamp at the start of epoch 0
    fn consensus_constants_checkpoint_zero_timestamp(&self) -> i64;

    /// Default period between epochs
    fn consensus_constants_checkpoints_period(&self) -> u16 {
        90
    }

    /// Default Hash value for the genesis block
    // TODO Decide an appropriate default value
    fn consensus_constants_genesis_hash(&self) -> Hash {
        Hash::SHA256([0; 32])
    }

    /// JSON-RPC server enabled by default
    fn jsonrpc_enabled(&self) -> bool {
        true
    }

    /// Default JSON-RPC server addr
    fn jsonrpc_server_address(&self) -> SocketAddr;

    /// MiningManager, enabled by default
    fn mining_enabled(&self) -> bool {
        true
    }

    /// Timeout for data request retrieval and aggregation execution
    fn mining_data_request_timeout(&self) -> Duration {
        // Default to 1/10 of the checkpoints period
        Duration::from_secs(u64::from(
            self.consensus_constants_checkpoints_period() / 10,
        ))
    }

    fn consensus_constants_max_block_weight(&self) -> u32 {
        // TODO: Replace  with real max_block_weight value used in mainnet
        10_000
    }

    /// Default number of seconds before giving up waiting for requested blocks: `400`.
    /// Sending 500 blocks should take less than 400 seconds.
    fn connections_blocks_timeout(&self) -> i64 {
        400
    }

    /// Default value for the maximum difference between timestamps in node handshaking
    fn connections_handshake_max_ts_diff(&self) -> i64 {
        10
    }

    /// An identity is considered active if it participated in the witnessing protocol at least once in the last `activity_period` epochs
    fn consensus_constants_activity_period(&self) -> u32 {
        // 1000 epochs at 90 seconds/epoch = 2 days
        //1000
        // 40 epochs = 2 hours
        40
    }

    /// Reputation will expire after N witnessing acts
    fn consensus_constants_reputation_expire_alpha_diff(&self) -> u32 {
        // 500 witnessing acts
        500
    }

    /// Reputation issuance
    fn consensus_constants_reputation_issuance(&self) -> u32 {
        // Issue 1 reputation point per witnessing act
        1
    }

    /// When to stop issuing new reputation
    fn consensus_constants_reputation_issuance_stop(&self) -> u32 {
        // Issue reputation points for the first 2^20 witnessing acts
        1 << 20
    }

    /// Penalization factor: fraction of reputation lost by liars for out of consensus claims
    fn consensus_constants_reputation_penalization_factor(&self) -> f64 {
        // Lose half of the total reputation for every lie
        0.5
    }

    /// Wallet server address
    fn wallet_server_addr(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 11212)
    }

    /// Wallet db file name
    fn wallet_db_file_name(&self) -> String {
        "witnet_wallet.db".to_string()
    }

    fn wallet_db_encrypt_hash_iterations(&self) -> u32 {
        10_000
    }

    fn wallet_db_encrypt_iv_length(&self) -> usize {
        16
    }

    fn wallet_db_encrypt_salt_length(&self) -> usize {
        32
    }

    fn wallet_seed_password(&self) -> ProtectedString {
        "".into()
    }

    fn wallet_master_key_salt(&self) -> Vec<u8> {
        b"Bitcoin seed".to_vec()
    }

    fn wallet_id_hash_iterations(&self) -> u32 {
        4096
    }

    fn wallet_id_hash_function(&self) -> HashFunction {
        HashFunction::Sha256
    }

    fn rocksdb_create_if_missing(&self) -> bool {
        true
    }

    fn rocksdb_compaction_readahead_size(&self) -> usize {
        0
    }

    fn rocksdb_use_fsync(&self) -> bool {
        false
    }

    fn rocksdb_enable_statistics(&self) -> bool {
        false
    }

    fn ntp_update_period(&self) -> Duration {
        Duration::from_secs(600)
    }

    fn ntp_server(&self) -> Vec<String> {
        vec![
            "0.pool.ntp.org:123".to_string(),
            "1.pool.ntp.org:123".to_string(),
            "2.pool.ntp.org:123".to_string(),
            "3.pool.ntp.org:123".to_string(),
        ]
    }

    fn ntp_enabled(&self) -> bool {
        true
    }

    fn mempool_tx_pending_timeout(&self) -> u64 {
        u64::from(self.consensus_constants_checkpoints_period()) * 10
    }
}

/// Struct that will implement all the mainnet defaults
pub struct Mainnet;

/// Struct that will implement all the testnet-1 defaults
pub struct Testnet1;

/// Struct that will implement all the testnet-3 defaults
pub struct Testnet3;

impl Defaults for Mainnet {
    fn connections_server_addr(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 11337)
    }

    fn jsonrpc_server_address(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 11338)
    }

    fn storage_db_path(&self) -> PathBuf {
        PathBuf::from(".witnet-rust-mainnet")
    }

    fn consensus_constants_checkpoint_zero_timestamp(&self) -> i64 {
        // A point far in the future, so the `EpochManager` will return an error
        // `EpochZeroInTheFuture`
        19_999_999_999_999
    }
}

impl Defaults for Testnet1 {
    fn connections_server_addr(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 21337)
    }

    fn jsonrpc_server_address(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 21338)
    }

    fn storage_db_path(&self) -> PathBuf {
        PathBuf::from(".witnet-rust-testnet-1")
    }

    fn connections_bootstrap_peers_period(&self) -> Duration {
        Duration::from_secs(15)
    }

    fn consensus_constants_checkpoint_zero_timestamp(&self) -> i64 {
        1_548_855_420
    }
}

impl Defaults for Testnet3 {
    fn connections_server_addr(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 21337)
    }

    fn jsonrpc_server_address(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 21338)
    }

    fn storage_db_path(&self) -> PathBuf {
        PathBuf::from(".witnet-rust-testnet-3")
    }

    fn connections_bootstrap_peers_period(&self) -> Duration {
        Duration::from_secs(15)
    }

    fn consensus_constants_checkpoint_zero_timestamp(&self) -> i64 {
        // June 1st, 2019
        1_559_347_200
    }
}
