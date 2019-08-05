use std::sync::Arc;

use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct IndexTxns(
    pub Arc<rocksdb::DB>,
    pub Vec<types::VTTransactionBody>,
    pub types::SimpleWallet,
);

impl Message for IndexTxns {
    type Result = ();
}

impl Handler<IndexTxns> for worker::Worker {
    type Result = <IndexTxns as Message>::Result;

    fn handle(
        &mut self,
        IndexTxns(db, txns, wallet): IndexTxns,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if let Err(err) = self.index_txns(worker::Db::new(db.as_ref()), &txns, &wallet) {
            log::error!("failed to index txns for wallet {}: {}", wallet.id, err);
        }
    }
}
