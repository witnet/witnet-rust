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
    /// Errors when registering sessions. Address in same IP range already registered in sessions
    #[fail(
        display = "Register failed. Address in same IP range ({}) already registered in sessions",
        range
    )]
    AddressInSameRangeAlreadyRegistered {
        /// A string representing the IP range for which a session already existed
        range: String,
    },
    /// Errors when unregistering sessions.
    #[fail(display = "Address could not be unregistered (not found in sessions)")]
    AddressNotFound,
    /// Errors when updating sessions
    #[fail(display = "Is not an outbound consolidated peer")]
    NotOutboundConsolidatedPeer,
    /// Errors when using SessionType::Feeler sessions in SessionsManager
    #[fail(display = "SessionsManager was told to manage a Feeler session.\
                      The session will be ignored because Feeler sessions should not be managed.")]
    NotExpectedFeelerPeer,
}

/// Sessions Errors under different operations
#[derive(Debug, PartialEq, Fail)]
pub enum PeersError {
    /// Peer not found. Empty buckets
    #[fail(display = "Peer not found. Empty buckets")]
    EmptyBuckets,
    /// Peer not found. Empty slot
    #[fail(display = "Peer not found. Empty slot")]
    EmptySlot,
}
