use actix::prelude::*;

use crate::{actors::worker, model, types};

pub struct GetUtxoInfo {
    pub wallet: types::SessionWallet,
}

impl Message for GetUtxoInfo {
    type Result = worker::Result<model::UtxoSet>;
}

impl Handler<GetUtxoInfo> for worker::Worker {
    type Result = <GetUtxoInfo as Message>::Result;

    fn handle(
        &mut self,
        GetUtxoInfo { wallet }: GetUtxoInfo,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.get_utxo_info(&wallet)
    }
}
