use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct GetVtt(
    pub types::SessionWallet,
    /// Key
    pub String,
);

impl Message for GetVtt {
    type Result = worker::Result<types::Transaction>;
}

impl Handler<GetVtt> for worker::Worker {
    type Result = <GetVtt as Message>::Result;

    fn handle(
        &mut self,
        GetVtt(wallet, transaction_hash): GetVtt,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.get_vtt(&wallet, transaction_hash)
    }
}
