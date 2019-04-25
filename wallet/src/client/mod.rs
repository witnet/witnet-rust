//! JsonRPC client

use actix::prelude::*;
use futures::{future, Future};
use jsonrpc_core as rpc;

use crate::err_codes;

mod actor;

/// Send request.
pub fn send(req: actor::Request) -> impl Future<Item = rpc::Value, Error = rpc::Error> {
    let rpc = actor::JsonRpc::from_registry();
    rpc.send(req).then(|resp| match resp {
        Ok(Ok(res)) => future::ok(res),
        Ok(Err(err)) => future::err(err),
        Err(e) => {
            log::error!("{}", e);
            let err = rpc::Error {
                code: rpc::ErrorCode::ServerError(err_codes::INTERNAL_ERROR),
                message: "Internal error".into(),
                data: None,
            };
            future::err(err)
        }
    })
}

/// Build a request object with the given method.
pub fn request<T: Into<String>>(method: T) -> actor::Request {
    actor::Request::method(method)
}
