//! TODO: doc
use std::fmt;

use jsonrpc_ws_server as server;

/// TODO: doc
#[derive(Debug)]
pub struct Error(pub(super) server::Error);

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "{}", self.0)
    }
}
