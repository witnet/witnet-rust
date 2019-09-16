// To enable `#[allow(clippy::all)]`
//#![feature(tool_lints)]

#![cfg_attr(test, allow(dead_code, unused_macros, unused_imports))]

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

/// Module containing transaction structures
pub mod transaction;

/// Module containing VRF-related structures
pub mod vrf;

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
    *ENVIRONMENT.read().unwrap()
}

/// Set the environment: mainnet or testnet.
/// This function should only be called once during initialization.
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

/// Set the environment: mainnet or testnet.
/// This function should only be called once during initialization.
#[cfg(test)]
pub fn set_environment(_environment: Environment) {
    panic!(
        "Dont set the environment in tests, as it can cause sporious failures: \
         multiple tests can run in parallel so some tests might fail when the \
         environment changes."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_environment() {
        assert_eq!(get_environment(), Environment::Mainnet);
    }

    #[test]
    #[should_panic]
    fn change_environment_in_tests() {
        set_environment(Environment::Mainnet);
    }
}
