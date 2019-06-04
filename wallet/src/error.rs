//! # Error definitions for the application.
//!
//! This module defines two types of errors:
//!
//! 1. `Error`: enum containing all possible errors that can occur during the handling of a request.
//! 2. `ApiError`: enum containing all errors that have a custom code in the JsonRPC protocol,
//! further information about the error is provided in the `data` field of the response.
use std::fmt;
use std::io;

use actix::MailboxError;
use jsonrpc_core as rpc;
use serde_json::{json, value::Value, Error as JsonError};

use crate::actors::storage::Error as StorageError;
use witnet_net::{client::tcp::jsonrpc::Error as ClientError, server::ws::Error as ServerError};
use witnet_rad::error::RadError;

/// Defines all the errors that can occur inside the application.
#[derive(Debug)]
pub enum Error {
    Mailbox(MailboxError),
    Storage(StorageError),
    Serialization(JsonError),
    Rad(RadError),
    Server(ServerError),
    Subscription(&'static str),
    Client(ClientError),
    NodeNotConnected,
    Io(io::Error),
}

/// Defines all the errors that have a custom code in the JsonRPC protocol.
#[derive(Debug)]
pub enum ApiError {
    Execution(Error),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::Mailbox(ref e) => write!(fmt, "mailbox error: {}", e),
            Error::Storage(ref e) => write!(fmt, "storage error: {}", e),
            Error::Serialization(ref e) => write!(fmt, "(de)serialization error: {}", e),
            Error::Rad(ref e) => write!(fmt, "rad error: {}", e),
            Error::Server(ref e) => write!(fmt, "server error: {}", e),
            Error::Client(ref e) => write!(fmt, "client error: {}", e),
            Error::Subscription(ref e) => write!(fmt, "{}", e),
            Error::Io(ref e) => write!(fmt, "{}", e),
            Error::NodeNotConnected => {
                write!(fmt, "the wallet currently is not connected to any node")
            }
        }
    }
}

impl Into<Value> for Error {
    fn into(self) -> Value {
        match &self {
            err => json!({ "code": self.code(), "message": format!("{}", err) }),
        }
    }
}

impl Error {
    fn code(&self) -> i64 {
        match self {
            _ => 100,
        }
    }
}
impl Into<rpc::Error> for ApiError {
    fn into(self) -> rpc::Error {
        match self {
            ApiError::Execution(err) => rpc::Error {
                code: rpc::ErrorCode::ServerError(1),
                message: "Execution Error.".into(),
                data: Some(err.into()),
            },
        }
    }
}
