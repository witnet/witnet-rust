use actix::prelude::*;

use crate::actors::worker;
use crate::{model, types};

pub struct GetAddresses {
    pub wallet: types::SessionWallet,
    /// Offset
    pub offset: u32,
    /// Limit
    pub limit: u32,
    /// External
    pub external: bool,
}

impl Message for GetAddresses {
    type Result = worker::Result<model::Addresses>;
}

impl Handler<GetAddresses> for worker::Worker {
    type Result = <GetAddresses as Message>::Result;

    fn handle(
        &mut self,
        GetAddresses {
            wallet,
            offset,
            limit,
            external,
        }: GetAddresses,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.addresses(&wallet, offset, limit, external)
    }
}
