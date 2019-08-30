use actix::prelude::*;

use crate::actors::worker;
use crate::{model, types};

pub struct IndexTxns(
    /// Wallet id
    pub String,
    pub types::SessionWallet,
    pub Vec<types::VTTransactionBody>,
    pub model::BlockInfo,
);

impl Message for IndexTxns {
    type Result = ();
}

impl Handler<IndexTxns> for worker::Worker {
    type Result = <IndexTxns as Message>::Result;

    fn handle(
        &mut self,
        IndexTxns(wallet_id, wallet, txns, block_info): IndexTxns,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if let Err(err) = self.index_txns(&wallet, &block_info, &txns) {
            log::warn!("failed to index txns for wallet {}: {}", wallet_id, err);
        }
    }
}
