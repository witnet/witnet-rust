use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct GetTransaction {
    pub wallet: types::SessionWallet,
    /// Transaction Id
    pub transaction_hash: String,
}

impl Message for GetTransaction {
    type Result = worker::Result<Option<types::Transaction>>;
}

impl Handler<GetTransaction> for worker::Worker {
    type Result = <GetTransaction as Message>::Result;

    fn handle(
        &mut self,
        GetTransaction {
            wallet,
            transaction_hash,
        }: GetTransaction,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.get_transaction(&wallet, transaction_hash)
    }
}
