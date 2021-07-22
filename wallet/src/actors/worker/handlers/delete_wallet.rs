use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct DeleteWallet {
    pub wallet: types::SessionWallet,
    pub wallet_id: String,
}

impl Message for DeleteWallet {
    type Result = worker::Result<()>;
}

impl Handler<DeleteWallet> for worker::Worker {
    type Result = <DeleteWallet as Message>::Result;

    fn handle(
        &mut self,
        DeleteWallet { wallet, wallet_id }: DeleteWallet,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.delete_wallet(&wallet, wallet_id)
    }
}
