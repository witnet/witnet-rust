use std::sync::Arc;

use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct GenAddress(
    pub Arc<rocksdb::DB>,
    pub types::Secret,
    pub String,
    pub Option<String>,
    pub types::ExtendedSK,
    pub u32,
    pub u32,
);

impl Message for GenAddress {
    type Result = worker::Result<String>;
}

impl Handler<GenAddress> for worker::Worker {
    type Result = <GenAddress as Message>::Result;

    fn handle(
        &mut self,
        GenAddress(db, enc_key, wallet_id, label, parent_key, account_index, key_index): GenAddress,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.gen_address(
            &worker::Db::new(db),
            enc_key.as_ref(),
            &wallet_id,
            label.as_ref(),
            &parent_key,
            account_index,
            key_index,
        )
    }
}
