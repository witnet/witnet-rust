//! Witnet data structures

#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
// FIXME: add the missing documentation and enable this lint
//#![deny(missing_docs)]
// FIXME: allow only for protobuf generated code
#![allow(elided_lifetimes_in_paths)]

#[macro_use]
extern crate protobuf_convert;

use std::sync::RwLock;

use lazy_static::lazy_static;

pub use strum;

use crate::{
    chain::{Environment, Epoch},
    proto::versioning::{ProtocolInfo, ProtocolVersion},
};

pub const DEFAULT_VALIDATOR_COUNT_FOR_TESTS: usize = 1000;

/// Module containing functions to generate Witnet's protocol messages
pub mod builders;

/// Module containing Witnet's chain data types
pub mod chain;

/// Module containing functions to convert between Witnet's protocol messages and Protocol Buffers
pub mod proto;

/// Module containing Witnet's protocol messages types
pub mod types;

/// Module containing error definitions
pub mod error;

/// Provides convenient constants, structs and methods for handling transaction fees.
pub mod fee;

/// Module containing data_request structures
pub mod data_request;

/// Module containing data structures for the staking functionality
pub mod staking;

/// Module containing superblock structures
pub mod superblock;

/// Module containing transaction structures
pub mod transaction;

/// High level transaction factory
pub mod transaction_factory;

/// Module containing UnspentOutputsPool and related structures
pub mod utxo_pool;

/// Module containing VRF-related structures
pub mod vrf;

/// Module containing definitions of RADON errors
pub mod radon_error;

/// Module containing RadonReport structures
pub mod radon_report;

/// Module containing structures related to witnessing
pub mod witnessing;

/// Serialization boilerplate to allow serializing some data structures as
/// strings or bytes depending on the serializer.
mod serialization_helpers;

/// Provides convenient constants, structs and methods for handling values denominated in Wit.
pub mod wit;

/// Provides support for segmented protocol capabilities.
pub mod capabilities;

lazy_static! {
    /// Environment in which we are running: mainnet or testnet.
    /// This is used for Bech32 serialization.
    // Default to mainnet so that external tools using the witnet_data_structures crate
    // can work without having to manually set the environment.
    // The default environment will also be used in tests.
    static ref ENVIRONMENT: RwLock<Environment> = RwLock::new(Environment::Mainnet);
    /// Protocol version that we are running.
    /// default to legacy for now — it's the v2 bootstrapping module's responsibility to upgrade it.
    static ref PROTOCOL: RwLock<ProtocolInfo> = RwLock::new(ProtocolInfo::default());
}

/// Environment in which we are running: mainnet or testnet.
pub fn get_environment() -> Environment {
    // This unwrap is safe as long as the lock is not poisoned.
    // The lock can only become poisoned when a writer panics.
    // The only writer is the one used in `set_environment`, which should only
    // be used during initialization, when there is only one thread running.
    // So a panic there should have stopped the node before this function
    // is ever called.
    *ENVIRONMENT.read().unwrap()
}

/// Set the environment: mainnet or testnet.
/// This function should only be called once during initialization.
// Changing the environment in tests is not supported, as it can cause spurious failures:
// multiple tests can run in parallel and some tests might fail when the environment changes.
// But if you need to change the environment in some test, just create a separate thread-local
// variable and mock get and set.
#[cfg(not(test))]
pub fn set_environment(environment: Environment) {
    match ENVIRONMENT.write() {
        Ok(mut x) => {
            *x = environment;
            log::debug!("Set environment to {}", environment);
        }
        Err(e) => {
            log::error!("Failed to set environment: {}", e);
        }
    }
}

/// Protocol version that we are running.
pub fn get_protocol_version(epoch: Option<Epoch>) -> ProtocolVersion {
    // This unwrap is safe as long as the lock is not poisoned.
    // The lock can only become poisoned when a writer panics.
    let protocol_info = PROTOCOL.read().unwrap();

    if let Some(epoch) = epoch {
        protocol_info.all_versions.version_for_epoch(epoch)
    } else {
        protocol_info.current_version
    }
}

/// Let the protocol versions controller know about a protocol version, and its activation epoch.
pub fn register_protocol_version(
    protocol_version: ProtocolVersion,
    epoch: Epoch,
    checkpoint_period: u16,
) {
    log::info!(
        "Registering protocol version {protocol_version}, which enters into force at epoch {epoch}"
    );
    // This unwrap is safe as long as the lock is not poisoned.
    // The lock can only become poisoned when a writer panics.
    let mut protocol_info = PROTOCOL.write().unwrap();
    protocol_info.register(epoch, protocol_version, checkpoint_period);
}

/// Set the protocol version that we are running.
pub fn set_protocol_version(protocol_version: ProtocolVersion) {
    log::info!("Setting current protocol version to {protocol_version}");
    // The lock can only become poisoned when a writer panics.
    let mut protocol = PROTOCOL.write().unwrap();
    protocol.current_version = protocol_version;
}

pub fn load_protocol_info(info: ProtocolInfo) {
    log::info!("Loading protocol versions data in bulk: {:?}", info);
    let mut protocol_info = PROTOCOL.write().unwrap();
    *protocol_info = info;
}

pub fn initialize_default(default_checkpoint_period: u16) {
    register_protocol_version(ProtocolVersion::default(), 0, default_checkpoint_period);
}

pub fn clear_protocol_info() {
    log::info!("Clearing all protocol versions");
    let mut protocol_info = PROTOCOL.write().unwrap();
    protocol_info.clear_versions();
}

/// Refresh the protocol version, i.e. derive the current version from the current epoch, and update `current_version`
/// accordingly.
pub fn refresh_protocol_version(current_epoch: Epoch) {
    let current_version = ProtocolVersion::from_epoch(current_epoch);
    set_protocol_version(current_version)
}

pub fn get_protocol_version_activation_epoch(protocol_version: ProtocolVersion) -> Epoch {
    // This unwrap is safe as long as the lock is not poisoned.
    // The lock can only become poisoned when a writer panics.
    let protocol = PROTOCOL.write().unwrap();
    protocol.all_versions.get_activation_epoch(protocol_version)
}

pub fn get_protocol_version_period(protocol_version: ProtocolVersion) -> u16 {
    // This unwrap is safe as long as the lock is not poisoned.
    // The lock can only become poisoned when a writer panics.
    let protocol = PROTOCOL.write().unwrap();
    match protocol.all_checkpoints_periods.get(&protocol_version) {
        Some(period) => *period,
        None => u16::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_environment() {
        // If this default changes, all the tests that rely on hardcoded
        // addresses serialized as Bech32 will fail
        assert_eq!(get_environment(), Environment::Mainnet);
    }

    #[test]
    fn protocol_versions() {
        // If this default changes before the transition to V2 is complete, almost everything will
        // break because data structures change schema and, serialization changes and hash
        // derivation breaks too
        let protocol_version = ProtocolVersion::guess();
        assert_eq!(protocol_version, ProtocolVersion::V1_7);

        // Register the different protocol versions
        register_protocol_version(ProtocolVersion::V1_7, 100, 45);
        register_protocol_version(ProtocolVersion::V1_8, 200, 45);
        register_protocol_version(ProtocolVersion::V2_0, 300, 20);

        // The initial protocol version should be the default one
        let version = ProtocolVersion::from_epoch(0);
        assert_eq!(version, ProtocolVersion::V1_7);

        // Right after the
        let version = ProtocolVersion::from_epoch(100);
        assert_eq!(version, ProtocolVersion::V1_7);
        let version = ProtocolVersion::from_epoch(200);
        assert_eq!(version, ProtocolVersion::V1_8);
        let version = ProtocolVersion::from_epoch(300);
        assert_eq!(version, ProtocolVersion::V2_0);

        let version = ProtocolVersion::guess();
        assert_eq!(version, ProtocolVersion::V1_7);

        set_protocol_version(ProtocolVersion::V2_0);

        let version = ProtocolVersion::guess();
        assert_eq!(version, ProtocolVersion::V2_0);
    }
}
