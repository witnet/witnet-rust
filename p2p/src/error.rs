//! Error type definitions for the Sessions module.

use thiserror::Error;

/// Sessions Errors under different operations
#[derive(Debug, PartialEq, Eq, Error)]
pub enum SessionsError {
    /// Errors when registering sessions. Max number of peers reached
    #[error("Register failed. Max number of peers reached")]
    MaxPeersReached,
    /// Errors when registering sessions. Address already registered in sessions
    #[error("Register failed. Address already registered in sessions")]
    AddressAlreadyRegistered,
    /// Errors when registering sessions. Address in same IP range already registered in sessions
    #[error("Register failed. Address in same IP range ({range}) already registered in sessions")]
    AddressInSameRangeAlreadyRegistered {
        /// A string representing the IP range for which a session already existed
        range: String,
    },
    /// Errors when unregistering sessions.
    #[error("Address could not be unregistered (not found in sessions)")]
    AddressNotFound,
    /// Errors when updating sessions
    #[error("Is not an outbound consolidated peer")]
    NotOutboundConsolidatedPeer,
    /// Errors when using SessionType::Feeler sessions in SessionsManager
    #[error(
        "SessionsManager was told to manage a Feeler session.\
            The session will be ignored because Feeler sessions should not be managed."
    )]
    NotExpectedFeelerPeer,
}

/// Sessions Errors under different operations
#[derive(Debug, PartialEq, Eq, Error)]
pub enum PeersError {
    /// Peer not found. Empty buckets
    #[error("Peer not found. Empty buckets")]
    EmptyBuckets,
    /// Peer not found. Empty slot
    #[error("Peer not found. Empty slot")]
    EmptySlot,
}
