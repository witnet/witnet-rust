//! Error type definitions for the Peers module.

use failure::Fail;
use std::fmt;
use witnet_util::error::WitnetResult;

/// Peers Error
#[derive(Debug, Fail)]
#[fail(display = "{} : at \"{}\", msg {}", kind, info, msg)]
pub struct PeersError {
    /// Operation kind
    kind: PeersErrorKind,
    /// Operation parameter
    info: String,
    /// Error message from database
    msg: String,
}

impl PeersError {
    /// Create a peers error based on operation kind and related info.
    pub fn new(kind: PeersErrorKind, info: String, msg: String) -> Self {
        Self { kind, info, msg }
    }
}

/// Peers Errors while operating on database
#[derive(Debug)]
pub enum PeersErrorKind {}

impl fmt::Display for PeersErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PeersError::{:?}", self)
    }
}

/// Result type for the peers module.
/// This is the only return type acceptable for any public method in a peers backend.
pub type PeersResult<T> = WitnetResult<T, PeersError>;
