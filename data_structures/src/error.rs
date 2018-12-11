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

/// Result type for the ChainInfo in ChainManager module.
pub type ChainInfoResult<T> = WitnetResult<T, ChainInfoError>;

/// Error in builders functions
#[derive(Debug, Fail)]
#[fail(display = "{} :  msg {}", kind, msg)]
pub struct BuildersError {
    /// Error kind
    kind: BuildersErrorKind,
    /// Error message
    msg: String,
}

impl BuildersError {
    /// Create a BuildersError based on kind and related info
    pub fn new(kind: BuildersErrorKind, msg: String) -> Self {
        Self { kind, msg }
    }
}

/// Kind of errors while trying to create a data structure with a builder function
#[derive(Debug)]
pub enum BuildersErrorKind {
    /// No inventory vectors available to create a message
    NoInvVectors,
}

impl fmt::Display for BuildersErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BuildersError::{:?}", self)
    }
}

/// Result type used as return value for the builder functions in the builders module
pub type BuildersResult<T> = WitnetResult<T, BuildersError>;
