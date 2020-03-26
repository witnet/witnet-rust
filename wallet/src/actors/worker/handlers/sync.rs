use actix::{Handler, Message};

use crate::actors::worker;
use crate::types;

pub struct SyncRequest {
    pub wallet_id: String,
    pub wallet: types::SessionWallet,
    pub since_epoch: u32,
}

impl Message for SyncRequest {
    type Result = worker::Result<()>;
}

impl Handler<SyncRequest> for worker::Worker {
    type Result = <SyncRequest as Message>::Result;

    fn handle(&mut self, msg: SyncRequest, _ctx: &mut Self::Context) -> Self::Result {
        self.sync(&msg.wallet_id, msg.wallet, msg.since_epoch)
    }
}
