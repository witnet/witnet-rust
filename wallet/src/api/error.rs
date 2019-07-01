use failure::Fail;
use jsonrpc_core as rpc;
use serde_json::json;

use crate::validation;

/// Possible errors returned by the API
pub enum Error {
    Validation(validation::Error),
    Internal(failure::Error),
    Node(failure::Error),
    Unauthorized,
    Forbidden,
}

impl Error {
    pub fn into_parts(self) -> (i64, &'static str, Option<serde_json::Value>) {
        match self {
            Error::Validation(e) => (
                400,
                "Validation Error",
                Some(serde_json::to_value(e).expect("serialization of errors failed")),
            ),
            Error::Unauthorized => (401, "Unauthorized", None),
            Error::Forbidden => (402, "Forbidden", None),
            Error::Node(e) => {
                log::error!("Node Error: {}", &e);
                (
                    510,
                    "Node Error",
                    Some(json!({ "cause": format!("{}", e) })),
                )
            }
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

/// Helper function to simplify .map_err on validation errors.
pub fn validation_error(err: validation::Error) -> Error {
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
