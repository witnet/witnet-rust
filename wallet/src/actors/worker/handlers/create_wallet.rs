use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct CreateWallet(
    /// Wallet name
    pub Option<String>,
    /// Wallet caption
    pub Option<String>,
    /// Wallet user-defined password
    pub types::Password,
    /// Seed data (mnemonics or xprv)
    pub types::SeedSource,
    /// Overwrite flag
    pub bool,
);

impl Message for CreateWallet {
    type Result = worker::Result<String>;
}

impl Handler<CreateWallet> for worker::Worker {
    type Result = <CreateWallet as Message>::Result;

    fn handle(
        &mut self,
        CreateWallet(name, caption, password, seed_source, overwrite): CreateWallet,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.create_wallet(name, caption, password.as_ref(), &seed_source, overwrite)
    }
}
