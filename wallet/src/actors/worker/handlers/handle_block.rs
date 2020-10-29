use actix::{Handler, Message};

use crate::actors::worker;
use crate::types;
use std::sync::Arc;

pub struct HandleBlockRequest {
    pub block: Arc<types::ChainBlock>,
    pub wallet: types::SessionWallet,
    pub sink: types::DynamicSink,
}

impl Message for HandleBlockRequest {
    type Result = worker::Result<()>;
}

impl Handler<HandleBlockRequest> for worker::Worker {
    type Result = <HandleBlockRequest as Message>::Result;

    fn handle(&mut self, msg: HandleBlockRequest, _ctx: &mut Self::Context) -> Self::Result {
        self.handle_block(msg.block, false, false, msg.wallet, msg.sink)
    }
}
