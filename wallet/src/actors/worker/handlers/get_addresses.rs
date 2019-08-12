use actix::prelude::*;

use crate::actors::worker;
use crate::model;

pub struct GetAddresses(
    pub model::WalletUnlocked,
    /// Offset
    pub u32,
    /// Limit
    pub u32,
);

impl Message for GetAddresses {
    type Result = worker::Result<model::Addresses>;
}

impl Handler<GetAddresses> for worker::Worker {
    type Result = <GetAddresses as Message>::Result;

    fn handle(
        &mut self,
        GetAddresses(wallet, offset, limit): GetAddresses,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.addresses(&wallet, offset, limit)
    }
}
