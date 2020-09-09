use actix::{Handler, Message};

use crate::actors::worker;
use crate::types;

pub struct HandleSuperBlockRequest {
    pub superblock_notification: types::SuperBlockNotification,
    pub wallet: types::SessionWallet,
    pub sink: types::DynamicSink,
}

impl Message for HandleSuperBlockRequest {
    type Result = worker::Result<()>;
}

impl Handler<HandleSuperBlockRequest> for worker::Worker {
    type Result = <HandleSuperBlockRequest as Message>::Result;

    fn handle(&mut self, msg: HandleSuperBlockRequest, _ctx: &mut Self::Context) -> Self::Result {
        self.handle_superblock(msg.superblock_notification, msg.wallet, msg.sink)
    }
}
