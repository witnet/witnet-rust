use actix::{Handler, Message};

use crate::actors::worker;
use crate::types;

pub struct Resync {
    pub wallet_id: String,
    pub wallet: types::SessionWallet,
    pub sink: types::DynamicSink,
}

impl Message for Resync {
    type Result = worker::Result<bool>;
}

impl Handler<Resync> for worker::Worker {
    type Result = <Resync as Message>::Result;

    fn handle(&mut self, msg: Resync, _ctx: &mut Self::Context) -> Self::Result {
        self.clear_chain_data_and_resync(&msg.wallet_id, msg.wallet, msg.sink)
    }
}
