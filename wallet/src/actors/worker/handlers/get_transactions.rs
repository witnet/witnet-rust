use actix::prelude::*;

use crate::actors::worker;
use crate::{model, types};

pub struct GetTransactions {
    pub wallet: types::SessionWallet,
    /// Offset
    pub offset: u32,
    /// Limit
    pub limit: u32,
}

impl Message for GetTransactions {
    type Result = worker::Result<model::WalletTransactions>;
}

impl Handler<GetTransactions> for worker::Worker {
    type Result = <GetTransactions as Message>::Result;

    fn handle(
        &mut self,
        GetTransactions {
            wallet,
            offset,
            limit,
        }: GetTransactions,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.transactions(&wallet, offset, limit)
    }
}
