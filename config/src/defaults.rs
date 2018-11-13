//! # Default per-environment values
//!
//! This module contains per-environment default values for the Witnet
//! protocol params.
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

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
}

/// Struct that will implement all the mainnet defaults
pub struct Mainnet;

/// Struct that will implement all the testnet-1 defaults
pub struct Testnet1;

impl Defaults for Mainnet {
    fn connections_server_addr(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 11337)
    }

    fn storage_db_path(&self) -> PathBuf {
        PathBuf::from(".witnet-rust-mainnet")
    }
}

impl Defaults for Testnet1 {
    fn connections_server_addr(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 21337)
    }

    fn storage_db_path(&self) -> PathBuf {
        PathBuf::from(".witnet-rust-testnet-1")
    }
}
