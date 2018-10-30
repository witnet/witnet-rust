//! Error type definitions for the Sessions module.

use failure::Fail;
use std::fmt;
use witnet_util::error::WitnetResult;

/// Sessions Error
#[derive(Debug, Fail)]
#[fail(display = "{} : at \"{}\", msg {}", kind, info, msg)]
pub struct SessionsError {
    /// Error kind
    kind: SessionsErrorKind,
    /// Error parameter
    info: String,
    /// Error message
    msg: String,
}

impl SessionsError {
    /// Create a sessions error based on operation kind and related info.
    pub fn new(kind: SessionsErrorKind, info: String, msg: String) -> Self {
        Self { kind, info, msg }
    }
}

/// Sessions Errors under different operations
#[derive(Debug)]
pub enum SessionsErrorKind {
    /// Errors when registering sessions
    Register,
    /// Errors when unregistering sessions
    Unregister,
    /// Errors when updating sessions
    Update,
}

impl fmt::Display for SessionsErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SessionsError::{:?}", self)
    }
}

/// Result type for the sessions module.
/// This is the only return type acceptable for any public method in a sessions backend.
pub type SessionsResult<T> = WitnetResult<T, SessionsError>;
