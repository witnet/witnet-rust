use actix::{Handler, Message};
use std::sync::Arc;

use crate::{actors::worker, types};
use witnet_data_structures::chain::Block;

pub struct HandleBlockRequest {
    pub block: Arc<Block>,
    pub wallet: types::SessionWallet,
    pub sink: types::DynamicSink,
}

impl Message for HandleBlockRequest {
    type Result = worker::Result<()>;
}

impl Handler<HandleBlockRequest> for worker::Worker {
    type Result = <HandleBlockRequest as Message>::Result;

    fn handle(&mut self, msg: HandleBlockRequest, _ctx: &mut Self::Context) -> Self::Result {
        self.handle_block(msg.block, false, msg.wallet, msg.sink)
    }
}
