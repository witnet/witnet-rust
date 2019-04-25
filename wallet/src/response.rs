//! Utilities related to JsonRPC responses.

use futures::Future;
use jsonrpc_core::Error;

/// TODO: doc
pub trait Response<I>: Future<Item = I, Error = Error> {}

impl<T, I> Response<I> for T where T: Future<Item = I, Error = Error> {}
