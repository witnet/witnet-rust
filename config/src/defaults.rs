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
        12
    }

    /// Default known peers: none
    fn connections_known_peers(&self) -> HashSet<SocketAddr> {
        HashSet::new()
    }

    /// Default path for the database
    fn storage_db_path(&self) -> PathBuf;

    /// Do not keep utxos in memory by default
    fn storage_utxos_in_memory(&self) -> bool {
        false
    }

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

    /// Default period for melt peers
    fn connections_check_melted_peers_period(&self) -> Duration {
        Duration::from_secs(300)
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
        60
    }

    /// Period in seconds for a potential peer address to be kept "iced", i.e. will not be tried
    /// again before that amount of time.
    fn connections_bucketing_ice_period(&self) -> Duration {
        Duration::from_secs(3600) // 4 hours
    }

    /// Period that indicate the validity of a checked peer
    fn connections_bucketing_update_period(&self) -> i64 {
        300
    }

    /// Reject (tarpit) inbound connections coming from addresses that are alike, so as
    /// to prevent sybil peers from monopolizing our inbound capacity.
    fn connections_reject_sybil_inbounds(&self) -> bool {
        true
    }

    /// Limit to reject (tarpit) inbound connections. If the limit is set to 18, the addresses having
    /// the same first 18 bits in the IP will collide, so as to prevent sybil peers from monopolizing our inbound capacity.
    fn connections_reject_sybil_inbounds_range_limit(&self) -> u8 {
        18
    }

    /// Limit the number of requested blocks that will be processed as one batch
    fn connections_requested_blocks_batch_limit(&self) -> u32 {
        // Default: 500 blocks
        500
    }

    /// Addresses to be used as proxies when performing data retrieval. No proxies are used by
    /// default.
    fn connections_retrieval_proxies(&self) -> Vec<String> {
        vec![]
    }

    /// Timestamp at the start of epoch 0
    fn consensus_constants_checkpoint_zero_timestamp(&self) -> i64;

    /// Default period between epochs
    fn consensus_constants_checkpoints_period(&self) -> u16 {
        45
    }

    /// Default period between superblocks
    fn consensus_constants_superblock_period(&self) -> u16 {
        10
    }

    /// Default period (in superblock periods) after which the committee should be reduced
    fn consensus_constants_superblock_committee_decreasing_period(&self) -> u32 {
        5
    }

    /// Default committee reduction step
    fn consensus_constants_superblock_committee_decreasing_step(&self) -> u32 {
        5
    }

    /// Default initial block reward
    fn consensus_constants_initial_block_reward(&self) -> u64 {
        // 250 wits
        250 * 1_000_000_000
    }

    /// Default halving period
    fn consensus_constants_halving_period(&self) -> u32 {
        // 3.5M epochs * (45 secs/epoch) ~> 5 years
        // This will be the first timestamp with halved reward:
        // 3_500_000 * 45 + 1_602_666_000 = 1_760_166_000
        // 2025-10-11 @ 7:00am (UTC)

        3_500_000
    }

    /// Default Hash value for the auxiliary bootstrap block
    fn consensus_constants_bootstrap_hash(&self) -> Hash {
        // Brrr
        "666564676f6573627272727c2f3030312f3738392f3432382f6130312e676966"
            .parse()
            .unwrap()
    }

    /// Default Hash value for the genesis block
    fn consensus_constants_genesis_hash(&self) -> Hash {
        "6ca267d9accde3336739331d42d63509b799c6431e8d02b2d2cc9d3943d7ab02"
            .parse()
            .unwrap()
    }

    /// Default size of the superblock signing committee
    fn consensus_constants_superblock_signing_committee_size(&self) -> u32 {
        100
    }

    /// JSON-RPC server enabled by default
    fn jsonrpc_enabled(&self) -> bool {
        true
    }

    /// Default JSON-RPC server addr
    fn jsonrpc_server_address(&self) -> SocketAddr;

    /// JSON-RPC sensitive methods enabled by default
    fn jsonrpc_enable_sensitive_methods(&self) -> bool {
        true
    }

    /// MiningManager, enabled by default
    fn mining_enabled(&self) -> bool {
        true
    }

    /// Timeout for data request retrieval and aggregation execution
    fn mining_data_request_timeout(&self) -> Duration {
        Duration::from_secs(2)
    }

    /// Set the limit of retrievals per epoch to 65_535.
    /// This in practice equals no limit enforcement.
    fn mining_data_request_max_retrievals_per_epoch(&self) -> u16 {
        core::u16::MAX
    }

    /// Genesis block path, "./genesis_block.json" by default
    fn mining_genesis_path(&self) -> String {
        "genesis_block.json".to_string()
    }

    /// Percentage to redistribute mint reward in another address
    fn mining_mint_external_percentage(&self) -> u8 {
        50
    }

    /// Mempool size limit in weight units
    fn mining_transactions_pool_total_weight_limit(&self) -> u64 {
        // Default limit: enough to fill 24 hours worth of blocks
        // With max_block_weight = 100_000 and block_period = 45, this is 192_000_000
        let seconds_in_one_hour = 60 * 60;
        let block_period = u64::from(self.consensus_constants_checkpoints_period());
        let max_block_weight = u64::from(
            self.consensus_constants_max_vt_weight() + self.consensus_constants_max_dr_weight(),
        );
        24 * seconds_in_one_hour * max_block_weight / block_period
    }

    /// Allow setting a minimum value transfer transaction fee to be included in a block
    /// Setting it to zero essentially means all VTT's can be included in a block
    fn mining_minimum_vtt_fee(&self) -> u64 {
        0
    }

    fn consensus_constants_max_vt_weight(&self) -> u32 {
        20_000
    }
    fn consensus_constants_max_dr_weight(&self) -> u32 {
        80_000
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
        // 2000 epochs = 50 hours with an epoch of 90 secs
        // 2000

        // Testnet configuration
        2000
    }

    /// Reputation will expire after N witnessing acts
    fn consensus_constants_reputation_expire_alpha_diff(&self) -> u32 {
        // 20_000 witnessing acts
        20_000
    }

    /// Reputation issuance
    fn consensus_constants_reputation_issuance(&self) -> u32 {
        // Issue 1 reputation point per witnessing act
        1
    }

    /// Minimum difficulty
    fn consensus_constants_minimum_difficulty(&self) -> u32 {
        2000
    }

    /// Epochs with minimum difficulty
    fn consensus_constants_epochs_with_minimum_difficulty(&self) -> u32 {
        2000
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

    /// Backup factor for mining: valid VRFs under this factor will result in broadcasting a block
    fn consensus_constants_mining_backup_factor(&self) -> u32 {
        8
    }

    /// Replication factor for mining: valid VRFs under this factor will have priority
    fn consensus_constants_mining_replication_factor(&self) -> u32 {
        3
    }

    /// Minimum value in nanowits for a collateral value
    fn consensus_constants_collateral_minimum(&self) -> u64 {
        // 1 wit = 1_000_000_000 nanowits
        1_000_000_000
    }

    /// Minimum input age of an UTXO for being a valid collateral
    fn consensus_constants_collateral_age(&self) -> u32 {
        1000
    }

    /// Number of extra rounds for commitments and reveals
    fn consensus_constants_extra_rounds(&self) -> u16 {
        3
    }

    /// First superblocks signing committee
    fn consensus_constants_bootstrapping_committee(&self) -> Vec<String> {
        vec![
            "wit1g0rkajsgwqux9rnmkfca5tz6djg0f87x7ms5qx".to_string(),
            "wit1cyrlc64hyu0rux7hclmg9rxwxpa0v9pevyaj2c".to_string(),
            "wit1asdpcspwysf0hg5kgwvgsp2h6g65y5kg9gj5dz".to_string(),
            "wit13l337znc5yuualnxfg9s2hu9txylntq5pyazty".to_string(),
            "wit17nnjuxmfuu92l6rxhque2qc3u2kvmx2fske4l9".to_string(),
            "wit1etherz02v4fvqty6jhdawefd0pl33qtevy7s4z".to_string(),
            "wit1drcpu0xc2akfcqn8r69vw70pj8fzjhjypdcfsq".to_string(),
            "wit1gxf0ca67vxtg27kkmgezg7dd84hwmzkxn7c62x".to_string(),
            "wit1hujx8v0y8rzqchmmagh8yw95r943cdddnegtgc".to_string(),
            "wit1yd97y52ezvhq4kzl6rph6d3v6e9yya3n0kwjyr".to_string(),
            "wit1fn5yxmgkphnnuu6347s2dlqpyrm4am280s6s9t".to_string(),
            "wit12khyjjk0s2hyuzyyhv5v2d5y5snws7l58z207g".to_string(),
        ]
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

    fn wallet_sync_address_batch_length(&self) -> u16 {
        20
    }

    fn wallet_use_unconfirmed_utxos(&self) -> bool {
        true
    }

    fn wallet_pending_transactions_timeout_seconds(&self) -> u64 {
        // Default: 10 epochs
        10 * u64::from(self.consensus_constants_checkpoints_period())
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
        Duration::from_secs(1024)
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

    fn mempool_max_reinserted_transactions(&self) -> u32 {
        100
    }
}

/// Struct that will implement all the development defaults
pub struct Development;

/// Struct that will implement all the mainnet defaults
pub struct Mainnet;

/// Struct that will implement all the testnet defaults
pub struct Testnet;

impl Defaults for Development {
    fn connections_server_addr(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 21337)
    }

    fn jsonrpc_server_address(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 21338)
    }

    fn storage_db_path(&self) -> PathBuf {
        PathBuf::from(".witnet")
    }

    fn connections_bootstrap_peers_period(&self) -> Duration {
        Duration::from_secs(15)
    }

    fn consensus_constants_checkpoint_zero_timestamp(&self) -> i64 {
        // Wednesday, 23-Sept-2020, 09:00 UTC
        1_600_851_600
    }

    fn connections_reject_sybil_inbounds(&self) -> bool {
        false
    }

    fn connections_reject_sybil_inbounds_range_limit(&self) -> u8 {
        0
    }
}

impl Defaults for Mainnet {
    fn connections_server_addr(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 11337)
    }

    fn jsonrpc_server_address(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 11338)
    }

    fn storage_db_path(&self) -> PathBuf {
        PathBuf::from(".witnet")
    }

    fn connections_bootstrap_peers_period(&self) -> Duration {
        Duration::from_secs(15)
    }

    fn consensus_constants_checkpoint_zero_timestamp(&self) -> i64 {
        // Wednesday, 14-Oct-2020, 09:00 UTC
        1_602_666_000
    }
}

impl Defaults for Testnet {
    fn connections_server_addr(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 21337)
    }

    fn jsonrpc_server_address(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 21338)
    }

    fn storage_db_path(&self) -> PathBuf {
        PathBuf::from(".witnet")
    }

    fn connections_bootstrap_peers_period(&self) -> Duration {
        Duration::from_secs(15)
    }

    fn consensus_constants_checkpoint_zero_timestamp(&self) -> i64 {
        // Wednesday, 23-Sept-2020, 09:00 UTC
        1_600_851_600
    }
}
