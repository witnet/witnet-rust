use actix::prelude::*;

use crate::actors::worker;
use crate::actors::worker::Error::AddressGeneration;
use crate::{model, types};

pub struct GenAddress(pub types::SessionWallet, pub bool, pub Option<String>);

impl Message for GenAddress {
    type Result = worker::Result<model::Address>;
}

impl Handler<GenAddress> for worker::Worker {
    type Result = <GenAddress as Message>::Result;

    fn handle(
        &mut self,
        GenAddress(wallet, external, label): GenAddress,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.gen_address(&wallet, external, label)
            .and_then(|addr_opt| {
                addr_opt.ok_or_else(|| {
                    AddressGeneration(
                        "Address cannot be generated if wallet was never unlocked".to_string(),
                    )
                })
            })
            .map(|addr| (*addr).clone())
    }
}
