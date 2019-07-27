use std::sync::Arc;

use actix::prelude::*;

use crate::actors::worker;
use crate::model;

pub struct GenAddress(
    pub Arc<rocksdb::DB>,
    pub model::WalletUnlocked,
    pub Option<String>,
);

impl Message for GenAddress {
    type Result = worker::Result<model::Address>;
}

impl Handler<GenAddress> for worker::Worker {
    type Result = <GenAddress as Message>::Result;

    fn handle(
        &mut self,
        GenAddress(db, wallet, label): GenAddress,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.gen_address(worker::Db::new(db.as_ref()), &wallet, label)
    }
}
