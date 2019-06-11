//! Error type definition
use failure::Fail;

use jsonrpc_ws_server as server;

/// Custom error type wrapping `jsonrpc_ws_server::Error` that implements `Fail`
#[derive(Debug, Fail)]
#[fail(display = "{}", _0)]
pub struct Error(#[fail(cause)] pub(super) server::Error);
