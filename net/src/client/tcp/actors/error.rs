//! Error type definition
use async_jsonrpc_client::ErrorKind as TransportErrorKind;
use failure::Fail;
use serde_json::{error::Error as JsonError, value, Value};

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
}
