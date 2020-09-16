use actix::prelude::*;

use crate::actors::worker;
use crate::{model, types};

pub struct GenAddress(pub types::SessionWallet, pub Option<String>);

impl Message for GenAddress {
    type Result = worker::Result<model::Address>;
}

impl Handler<GenAddress> for worker::Worker {
    type Result = <GenAddress as Message>::Result;

    fn handle(
        &mut self,
        GenAddress(wallet, label): GenAddress,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.gen_address(&wallet, label)
            .map(|address| (*address).clone())
    }
}
