use std::sync::Arc;

use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct CreateWallet(
    pub Arc<rocksdb::DB>,
    /// Wallet name
    pub Option<String>,
    /// Wallet caption
    pub Option<String>,
    pub types::Password,
    pub types::SeedSource,
);

impl Message for CreateWallet {
    type Result = worker::Result<()>;
}

impl Handler<CreateWallet> for worker::Worker {
    type Result = <CreateWallet as Message>::Result;

    fn handle(
        &mut self,
        CreateWallet(db, name, caption, password, seed_source): CreateWallet,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.create_wallet(
            &mut worker::Db::new(db),
            name,
            caption,
            password.as_ref(),
            seed_source,
        )
    }
}
