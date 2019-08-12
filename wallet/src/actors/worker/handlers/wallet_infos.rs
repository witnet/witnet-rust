use actix::prelude::*;

use crate::actors::worker;
use crate::model;

pub struct WalletInfos;

impl Message for WalletInfos {
    type Result = worker::Result<Vec<model::Wallet>>;
}

impl Handler<WalletInfos> for worker::Worker {
    type Result = <WalletInfos as Message>::Result;

    fn handle(&mut self, _msg: WalletInfos, _ctx: &mut Self::Context) -> Self::Result {
        self.wallet_infos()
    }
}
