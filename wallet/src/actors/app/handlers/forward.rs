//! # Handler for forward messages
///
/// Forward messages are sent directly to the node without any processing by the wallet.
/// See `Forward` struct for more info.
use actix::prelude::*;
use jsonrpc_core as rpc;
use serde_json as json;

use crate::actors::App;
use crate::error;

/// Forward message. It will send a JsonRPC request with the given method string and params to the
/// node.
pub struct Forward(pub String, pub rpc::Params);

impl Message for Forward {
    type Result = Result<json::Value, error::Error>;
}

impl Handler<Forward> for App {
    type Result = ResponseFuture<json::Value, error::Error>;

    fn handle(
        &mut self,
        Forward(method, params): Forward,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.forward(method, params)
    }
}
