use std::sync::Arc;

use actix::prelude::*;

use crate::actors::worker;
use crate::{model, types};

pub struct AccountBalance(pub Arc<rocksdb::DB>, pub types::SimpleWallet);

impl Message for AccountBalance {
    type Result = worker::Result<model::AccountBalance>;
}

impl Handler<AccountBalance> for worker::Worker {
    type Result = <AccountBalance as Message>::Result;

    fn handle(
        &mut self,
        AccountBalance(db, wallet): AccountBalance,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.account_balance(worker::Db::new(db.as_ref()), &wallet)
            .map(|balance| model::AccountBalance {
                wallet_id: wallet.id,
                account: wallet.account_index,
                balance,
            })
    }
}
