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
    /// Protocol epoch in which a wallet was created (won't synchronize blocks prior to this epoch)
    pub birth_date: Option<types::BirthDate>,
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
            birth_date,
        }: CreateWallet,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.create_wallet(
            name,
            description,
            password.as_ref(),
            &seed_source,
            overwrite,
            birth_date,
        )
    }
}
