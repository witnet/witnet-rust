use actix::prelude::*;

use crate::actors::worker;
use crate::{model, types};

pub struct GetTransactions(
    pub types::SessionWallet,
    /// Offset
    pub u32,
    /// Limit
    pub u32,
);

impl Message for GetTransactions {
    type Result = worker::Result<model::Transactions>;
}

impl Handler<GetTransactions> for worker::Worker {
    type Result = <GetTransactions as Message>::Result;

    fn handle(
        &mut self,
        GetTransactions(wallet, offset, limit): GetTransactions,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.transactions(&wallet, offset, limit)
    }
}
