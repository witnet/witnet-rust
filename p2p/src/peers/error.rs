//! Error type definitions for the Peers module.

use failure::Fail;
use std::fmt;
use witnet_util::error::WitnetResult;

/// Peers management error
#[derive(Debug, Fail)]
#[fail(display = "{} :  msg {}", kind, msg)]
pub struct PeersError {
    /// Which operation errored
    kind: PeersErrorKind,
    /// Error message
    msg: String,
}

impl PeersError {
    /// Create a peers management error based on operation kind and related message
    pub fn new(kind: PeersErrorKind, msg: String) -> Self {
        Self { kind, msg }
    }
}

/// Different kinds of peers management errors
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
