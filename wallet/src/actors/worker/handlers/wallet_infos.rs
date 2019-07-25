use std::sync::Arc;

use actix::prelude::*;

use crate::actors::worker;
use crate::model;

pub struct WalletInfos(pub Arc<rocksdb::DB>);

impl Message for WalletInfos {
    type Result = worker::Result<Vec<model::WalletInfo>>;
}

impl Handler<WalletInfos> for worker::Worker {
    type Result = <WalletInfos as Message>::Result;

    fn handle(&mut self, WalletInfos(db): WalletInfos, _ctx: &mut Self::Context) -> Self::Result {
        self.wallet_infos(&worker::Db::new(db.as_ref()))
    }
}
