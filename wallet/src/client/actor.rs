//! JsonRPC client that communicates the wallet with the node.

use actix::prelude::*;
use async_jsonrpc_client::{
    transports::{shared::EventLoopHandle, tcp::TcpSocket},
    Transport,
};
use futures::Future;
use jsonrpc_core as rpc;

use crate::error;

/// JsonRPC client.
#[derive(Debug)]
pub struct JsonRpc {
    handle: EventLoopHandle,
    s: TcpSocket,
    // subscriptions: Subscriptions,
    // node_id_to_client_id: HashMap<SubscriptionId, SubscriptionId>,
}

impl JsonRpc {
    fn new(url: &str) -> Self {
        let (handle, s) = TcpSocket::new(url).unwrap();
        Self {
            handle,
            s,
            // subscriptions: Default::default(),
            // node_id_to_client_id: Default::default(),
        }
    }
}

impl Default for JsonRpc {
    fn default() -> Self {
        Self::new("127.0.0.1:1234")
    }
}

impl Actor for JsonRpc {
    type Context = Context<Self>;
}

impl Supervised for JsonRpc {}

impl SystemService for JsonRpc {}

/// A JsonRPC request.
pub struct Request {
    pub method: String,
    pub params: rpc::Params,
}

#[allow(dead_code)]
impl Request {
    /// Create a new request with the given method.
    pub fn method<T: Into<String>>(method: T) -> Self {
        Self {
            method: method.into(),
            params: rpc::Params::None,
        }
    }

    /// Set request params
    pub fn params(mut self, params: rpc::Params) -> Self {
        self.params = params;
        self
    }
}

impl Message for Request {
    type Result = Result<rpc::Value, rpc::Error>;
}

impl Handler<Request> for JsonRpc {
    type Result = ResponseFuture<rpc::Value, rpc::Error>;

    fn handle(&mut self, msg: Request, _ctx: &mut Context<Self>) -> Self::Result {
        let method = msg.method;
        let params = match msg.params {
            rpc::Params::None => rpc::Value::Null,
            rpc::Params::Array(items) => rpc::Value::Array(items),
            rpc::Params::Map(items) => rpc::Value::Object(items),
        };
        log::debug!("Calling node method {} with params {}", method, params);

        let fut = self.s.execute(&method, params.clone()).map_err(|err| {
            log::error!("Error received from node: {}", err);
            error::ApiError::Node(err).into()
        });

        Box::new(fut)
    }
}
