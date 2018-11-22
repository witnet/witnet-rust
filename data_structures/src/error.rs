//! Error type definitions for the data structure module.

use failure::Fail;
use std::fmt;
use witnet_util::error::WitnetResult;

/// Storage Error
#[derive(Debug, Fail)]
#[fail(display = "{} :  msg {}", kind, msg)]
pub struct ChainInfoError {
    /// Operation kind
    kind: ChainInfoErrorKind,
    /// Error message
    msg: String,
}

impl ChainInfoError {
    /// Create a storage error based on operation kind and related info.
    pub fn new(kind: ChainInfoErrorKind, msg: String) -> Self {
        Self { kind, msg }
    }
}

/// Chain Info Errors while operating on database
#[derive(Debug)]
pub enum ChainInfoErrorKind {
    /// Errors when try to use a None value for ChainInfo
    ChainInfoNotFound,
}

impl fmt::Display for ChainInfoErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChainInfo::{:?}", self)
    }
}

/// Result type for the ChainInfo in BlocksManager module.
pub type ChainInfoResult<T> = WitnetResult<T, ChainInfoError>;
