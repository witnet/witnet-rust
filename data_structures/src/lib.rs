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

use crate::chain::Environment;
use lazy_static::lazy_static;
use std::sync::RwLock;

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

/// Module containing data_request structures
pub mod data_request;

/// Module containing post mainnet validations
pub mod mainnet_validations;

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

lazy_static! {
    /// Environment in which we are running: mainnet or testnet.
    /// This is used for Bech32 serialization.
    // Default to mainnet so that external tools using the witnet_data_structures crate
    // can work without having to manually set the environment.
    // The default environment will also be used in tests.
    static ref ENVIRONMENT: RwLock<Environment> = RwLock::new(Environment::Mainnet);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_environment() {
        // If this default changes, all the tests that rely on hardcoded
        // addresses serialized as Bech32 will fail
        assert_eq!(get_environment(), Environment::Mainnet);
    }
}
