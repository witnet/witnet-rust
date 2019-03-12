//! # Default per-environment values
//!
//! This module contains per-environment default values for the Witnet
//! protocol params.
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use witnet_data_structures::chain::Hash;

// When changing the defaults, remember to update the documentation!
// https://github.com/witnet/witnet-rust/blob/master/docs/configuration/toml-file.md
// https://github.com/witnet/witnet-rust/blob/master/docs/configuration/environment.md

/// Trait defining all the configuration params that have a suitable
/// default value depending on the environment (mainnet, testnet,
/// etc).
pub trait Defaults {
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

    /// Default handshake timeout
    fn connections_handshake_timeout(&self) -> Duration {
        Duration::from_secs(5)
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

    /// Default demurrage value for reputation algorithm
    // TODO Decide an appropriate default value
    fn consensus_constants_reputation_demurrage(&self) -> f64 {
        0.0
    }

    /// Default punishment value for reputation algorithm
    // TODO Decide an appropriate default value
    fn consensus_constants_reputation_punishment(&self) -> f64 {
        0.0
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

    fn consensus_constants_max_block_weight(&self) -> u32 {
        // TODO: Replace  with real max_block_weight value used in mainnet
        10_000
    }

    /// Default number of seconds before giving up waiting for requested blocks: `400`.
    /// Sending 500 blocks should take less than 400 seconds.
    fn connections_blocks_timeout(&self) -> i64 {
        400
    }
}

/// Struct that will implement all the mainnet defaults
pub struct Mainnet;

/// Struct that will implement all the testnet-1 defaults
pub struct Testnet1;

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
        // A point far in the future, so the `EpochManager` will return an error
        // `EpochZeroInTheFuture`
        1_548_855_420
    }
}
