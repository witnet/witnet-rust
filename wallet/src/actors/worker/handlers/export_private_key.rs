use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct ExportPrivateKey(pub types::SessionWallet, pub types::Password);

impl Message for ExportPrivateKey {
    type Result = worker::Result<String>;
}

impl Handler<ExportPrivateKey> for worker::Worker {
    type Result = <ExportPrivateKey as Message>::Result;

    fn handle(
        &mut self,
        ExportPrivateKey(wallet, password): ExportPrivateKey,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.export_private_key(&wallet, password.as_ref())
    }
}
