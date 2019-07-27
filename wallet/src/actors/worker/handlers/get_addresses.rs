use std::sync::Arc;

use actix::prelude::*;

use crate::actors::worker;
use crate::model;

pub struct GetAddresses(
    pub Arc<rocksdb::DB>,
    pub model::WalletUnlocked,
    /// Offset
    pub u32,
    /// Limit
    pub u32,
);

impl Message for GetAddresses {
    type Result = worker::Result<model::Addresses>;
}

impl Handler<GetAddresses> for worker::Worker {
    type Result = <GetAddresses as Message>::Result;

    fn handle(
        &mut self,
        GetAddresses(db, wallet, offset, limit): GetAddresses,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.addresses(worker::Db::new(db.as_ref()), &wallet, offset, limit)
    }
}
