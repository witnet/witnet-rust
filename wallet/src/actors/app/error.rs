use jsonrpc_core as rpc;
use serde_json::json;
use thiserror::Error;

use witnet_net::client::tcp;

use crate::{actors, crypto, repository};

#[derive(Debug, Error)]
pub enum Error {
    #[error("validation error ({0:?})")]
    Validation(ValidationErrors),
    #[error("internal error: {0}")]
    Internal(anyhow::Error),
    #[error("JsonRPC timeout error")]
    JsonRpcTimeout,
    #[error("node error: {0}")]
    Node(anyhow::Error),
    #[error("wallet is not connected to a node")]
    NodeNotConnected,
    #[error("session not found")]
    SessionNotFound,
    #[error("session(s) are still open")]
    SessionsStillOpen,
    #[error("wallet not found")]
    WalletNotFound,
    #[error("wallet with id {0} already exists")]
    WalletAlreadyExists(String),
}

impl Error {
    pub fn into_parts(self) -> (i64, &'static str, Option<serde_json::Value>) {
        match &self {
            Error::Validation(e) => (
                400,
                "Validation Error",
                Some(serde_json::to_value(e).expect("serialization of errors failed")),
            ),
            Error::SessionNotFound => (401, "Unauthorized", None),
            Error::WalletNotFound => (402, "Forbidden", None),
            Error::WalletAlreadyExists(wallet_id) => (
                409,
                "Wallet Conflict",
                Some(json!({ "cause": self.to_string(), "wallet_id": wallet_id })),
            ),
            Error::Node(e) => {
                log::error!("Node Error: {}", &e);
                (
                    510,
                    "Node Error",
                    Some(json!({ "cause": format!("{}", e) })),
                )
            }
            Error::JsonRpcTimeout => {
                log::error!("Timeout Error");
                (408, "Timeout Error", None)
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
            Error::SessionsStillOpen => (401, "Unauthorized", None),
        }
    }
}

/// Helper function to simplify .map_err on validation errors.
pub fn validation_error(err: ValidationErrors) -> Error {
    Error::Validation(err)
}

/// Helper function to simplify .map_err on internal errors.
pub fn internal_error<T: std::error::Error + Send + Sync + 'static>(err: T) -> Error {
    Error::Internal(anyhow::Error::from(err))
}

/// Helper function to simplify .map_err on node errors.
pub fn node_error<T: std::error::Error + Send + Sync + 'static>(err: T) -> Error {
    Error::Node(anyhow::Error::from(err))
}

impl From<Error> for rpc::Error {
    fn from(x: Error) -> Self {
        let (code, message, data) = x.into_parts();
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
        match err {
            actors::worker::Error::WalletAlreadyExists(e) => Error::WalletAlreadyExists(e),
            actors::worker::Error::WrongPassword => {
                validation_error(field_error("password", "Wrong password"))
            }
            actors::worker::Error::WalletNotFound => {
                validation_error(field_error("wallet_id", "Wallet not found"))
            }
            actors::worker::Error::Node(e) => Error::Node(e),
            actors::worker::Error::KeyGen(e @ crypto::Error::InvalidKeyPath(_)) => {
                validation_error(field_error("seedData", e.to_string()))
            }
            actors::worker::Error::Repository(repository::Error::InsufficientBalance {
                total_balance,
                available_balance,
                transaction_value,
            }) => validation_error(field_error(
                json! {{
                    "total_balance": total_balance,
                    "available_balance": available_balance,
                    "transaction_value": transaction_value,
                }},
                "Wallet account has not enough balance",
            )),
            actors::worker::Error::JsonRpcTimeout => Error::JsonRpcTimeout,
            _ => internal_error(err),
        }
    }
}

impl From<tcp::Error> for Error {
    fn from(err: tcp::Error) -> Self {
        node_error(err)
    }
}

/// A list of errors. An error is a pair of (field, error msg).
pub type ValidationErrors = Vec<(String, String)>;

/// Create an error message associated to a field name.
pub fn field_error<F: ToString, M: ToString>(field: F, msg: M) -> ValidationErrors {
    vec![(field.to_string(), msg.to_string())]
}

/// Combine two Results but accumulate their errors.
pub fn combine_field_errors<A, B, C, F>(
    res1: std::result::Result<A, ValidationErrors>,
    res2: std::result::Result<B, ValidationErrors>,
    combinator: F,
) -> std::result::Result<C, ValidationErrors>
where
    F: FnOnce(A, B) -> C,
{
    match (res1, res2) {
        (Err(mut err1), Err(err2)) => {
            err1.extend(err2);
            Err(err1)
        }
        (Err(err1), _) => Err(err1),
        (_, Err(err2)) => Err(err2),
        (Ok(a), Ok(b)) => Ok(combinator(a, b)),
    }
}
