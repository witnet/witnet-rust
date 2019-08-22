use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct IndexTxns(
    pub String,
    pub types::SessionWallet,
    pub Vec<types::VTTransactionBody>,
);

impl Message for IndexTxns {
    type Result = ();
}

impl Handler<IndexTxns> for worker::Worker {
    type Result = <IndexTxns as Message>::Result;

    fn handle(
        &mut self,
        IndexTxns(wallet_id, wallet, txns): IndexTxns,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if let Err(err) = self.index_txns(&wallet, &txns) {
            log::warn!("failed to index txns for wallet {}: {}", wallet_id, err);
        }
    }
}
