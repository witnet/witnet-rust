use actix::prelude::*;

use crate::actors::worker;
use crate::{model, types};

pub struct GetBalance {
    pub wallet: types::SessionWallet,
}

impl Message for GetBalance {
    type Result = worker::Result<model::WalletBalance>;
}

impl Handler<GetBalance> for worker::Worker {
    type Result = <GetBalance as Message>::Result;

    fn handle(
        &mut self,
        GetBalance { wallet }: GetBalance,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.balance(&wallet)
    }
}
