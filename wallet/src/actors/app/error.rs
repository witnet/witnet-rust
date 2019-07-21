use failure::Fail;
use jsonrpc_core as rpc;
use serde_json::json;

use witnet_net::client::tcp;

use super::*;
use crate::actors;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "validation error")]
    Validation(ValidationErrors),
    #[fail(display = "internal error")]
    Internal(failure::Error),
    #[fail(display = "node error")]
    Node(failure::Error),
    #[fail(display = "wallet is not connected to a node")]
    NodeNotConnected,
    #[fail(display = "session not found")]
    SessionNotFound,
    #[fail(display = "wallet not found")]
    WalletNotFound,
}

impl Error {
    pub fn into_parts(self) -> (i64, &'static str, Option<serde_json::Value>) {
        match self {
            Error::Validation(e) => (
                400,
                "Validation Error",
                Some(serde_json::to_value(e).expect("serialization of errors failed")),
            ),
            Error::SessionNotFound => (401, "Unauthorized", None),
            Error::WalletNotFound => (402, "Forbidden", None),
            Error::Node(e) => {
                log::error!("Node Error: {}", &e);
                (
                    510,
                    "Node Error",
                    Some(json!({ "cause": format!("{}", e) })),
                )
            }
            Error::NodeNotConnected => (520, "Node Not Connected", None),
            Error::Internal(e) => {
                log::error!("Internal Error: {}", &e);
                (
                    500,
                    "Internal Error",
                    Some(json!({ "cause": format!("{}", e) })),
                )
            }
        }
    }
}

/// Helper function to simplify .map_err on validation errors.
pub fn validation_error(err: ValidationErrors) -> Error {
    Error::Validation(err)
}

/// Helper function to simplify .map_err on internal errors.
pub fn internal_error<T: Fail>(err: T) -> Error {
    Error::Internal(failure::Error::from(err))
}

/// Helper function to simplify .map_err on node errors.
pub fn node_error<T: Fail>(err: T) -> Error {
    Error::Node(failure::Error::from(err))
}

impl Into<rpc::Error> for Error {
    fn into(self) -> rpc::Error {
        let (code, message, data) = self.into_parts();
        rpc::Error {
            code: rpc::ErrorCode::ServerError(code),
            message: message.into(),
            data,
        }
    }
}

impl From<actix::MailboxError> for Error {
    fn from(err: actix::MailboxError) -> Self {
        internal_error(err)
    }
}

impl From<actors::worker::Error> for Error {
    fn from(err: actors::worker::Error) -> Self {
        internal_error(err)
    }
}

impl From<tcp::Error> for Error {
    fn from(err: tcp::Error) -> Self {
        node_error(err)
    }
}
