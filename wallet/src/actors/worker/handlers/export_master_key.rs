use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct ExportMasterKey {
    pub wallet: types::SessionWallet,
    pub password: types::Password,
}

impl Message for ExportMasterKey {
    type Result = worker::Result<String>;
}

impl Handler<ExportMasterKey> for worker::Worker {
    type Result = <ExportMasterKey as Message>::Result;

    fn handle(
        &mut self,
        ExportMasterKey { wallet, password }: ExportMasterKey,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.export_master_key(&wallet, password)
    }
}
