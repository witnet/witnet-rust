use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct CreateWallet {
    /// Wallet name
    pub name: Option<String>,
    /// Wallet description
    pub description: Option<String>,
    /// Wallet user-defined password
    pub password: types::Password,
    /// Seed data (mnemonics or xprv)
    pub seed_source: types::SeedSource,
    /// Overwrite flag
    pub overwrite: bool,
}

impl Message for CreateWallet {
    type Result = worker::Result<String>;
}

impl Handler<CreateWallet> for worker::Worker {
    type Result = <CreateWallet as Message>::Result;

    fn handle(
        &mut self,
        CreateWallet {
            name,
            description,
            password,
            seed_source,
            overwrite,
        }: CreateWallet,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.create_wallet(
            name,
            description,
            password.as_ref(),
            &seed_source,
            overwrite,
        )
    }
}
