//! Error type definitions for the Sessions module.

use failure::Fail;

/// Sessions Errors under different operations
#[derive(Debug, PartialEq, Fail)]
pub enum SessionsError {
    /// Errors when registering sessions. Max number of peers reached
    #[fail(display = "Register failed. Max number of peers reached")]
    MaxPeersReached,
    /// Errors when registering sessions. Address already registered in sessions
    #[fail(display = "Register failed. Address already registered in sessions")]
    AddressAlreadyRegistered,
    /// Errors when unregistering sessions.
    #[fail(display = "Address could not be unregistered (not found in sessions)")]
    AddressNotFound,
    /// Errors when updating sessions
    #[fail(display = "Is not an outbound consolidated peer")]
    NotOutboundConsolidatedPeer,
}
