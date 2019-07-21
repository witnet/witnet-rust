//! Error type definition
use async_jsonrpc_client::ErrorKind as TransportErrorKind;
use failure::Fail;
use serde_json::error::Error as JsonError;

/// Possible types of errors that can occurr when sending requests.
#[derive(Debug, Fail)]
pub enum Error {
    /// The url used to create the connection is not valid.
    #[fail(display = "couldn't start client due to invalid url")]
    InvalidUrl,
    /// The error ocurred at the transport layer, e.g.: connection or event loop might be down.
    #[fail(display = "request failed")]
    RequestFailed {
        /// The source of the error.
        error_kind: TransportErrorKind,
    },
    /// The error ocurred when serializaing the request params to json.
    #[fail(display = "request params failed to serialize to json")]
    SerializeFailed(#[cause] JsonError),
    /// The request timed out after the given duration.
    #[fail(display = "request timed out after {} milliseconds", _0)]
    RequestTimedOut(u128),
    /// The actor is not reachable.
    #[fail(display = "{}", _0)]
    Mailbox(#[cause] actix::MailboxError),
}

impl From<actix::MailboxError> for Error {
    fn from(err: actix::MailboxError) -> Self {
        Error::Mailbox(err)
    }
}
