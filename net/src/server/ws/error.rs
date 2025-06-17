//! Error type definition
use thiserror::Error;

use jsonrpc_ws_server as server;

/// Custom error type wrapping `jsonrpc_ws_server::Error` that implements `Fail`
#[derive(Debug, Error)]
#[error("{0}")]
pub struct Error(pub(super) server::Error);
