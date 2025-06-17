//! Error type definition
use async_jsonrpc_client::ErrorKind as TransportErrorKind;
use serde_json::error::Error as JsonError;
use thiserror::Error;

/// Possible types of errors that can occurr when sending requests.
#[derive(Debug, Error)]
pub enum Error {
    /// No url has been provided.
    #[error("couldn't start client because no url was provided")]
    NoUrl,
    /// The url used to create the connection is not valid.
    #[error("couldn't start client due to invalid url")]
    InvalidUrl,
    /// The error ocurred at the transport layer, e.g.: connection or event loop might be down.
    #[error("request failed: {message}")]
    RequestFailed {
        /// The source of the error.
        error_kind: TransportErrorKind,
        /// Error message.
        message: String,
    },
    /// The error ocurred when serializaing the request params to json.
    #[error("request params failed to serialize to json")]
    SerializeFailed(JsonError),
    /// The request timed out after the given duration.
    #[error("request timed out after {0} milliseconds")]
    RequestTimedOut(u128),
    /// The actor is not reachable.
    #[error("{0}")]
    Mailbox(actix::MailboxError),
}

impl From<actix::MailboxError> for Error {
    fn from(err: actix::MailboxError) -> Self {
        Error::Mailbox(err)
    }
}
