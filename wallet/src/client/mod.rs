//! JsonRPC client

use actix::prelude::*;
use futures::{future, Future};
use jsonrpc_core as rpc;

use crate::error;

mod actor;

/// Send request.
pub fn send(req: actor::Request) -> impl Future<Item = rpc::Value, Error = rpc::Error> {
    let rpc = actor::JsonRpc::from_registry();
    rpc.send(req).then(|resp| match resp {
        Ok(Ok(res)) => future::ok(res),
        Ok(Err(err)) => future::err(err),
        Err(err) => {
            log::error!("{}", err);
            future::err(error::ApiError::Execution(error::Error::Mailbox(err)).into())
        }
    })
}

/// Build a request object with the given method.
pub fn request<T: Into<String>>(method: T) -> actor::Request {
    actor::Request::method(method)
}
