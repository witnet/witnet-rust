use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct NotifyBalance(pub types::SessionWallet, pub types::Sink);

impl Message for NotifyBalance {
    type Result = ();
}

impl Handler<NotifyBalance> for worker::Worker {
    type Result = <NotifyBalance as Message>::Result;

    fn handle(
        &mut self,
        NotifyBalance(wallet, sink): NotifyBalance,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if let Err(err) = self.notify_balance(&wallet, &sink) {
            log::warn!("failed to notify balance of wallet: {}", err);
        }
    }
}
