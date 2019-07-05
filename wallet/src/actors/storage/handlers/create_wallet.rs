use actix::prelude::*;

use crate::actors::storage::Storage;
use crate::{storage::Error, types};

pub struct CreateWallet(pub types::SharedDB, pub types::Wallet, pub types::Password);

impl Message for CreateWallet {
    type Result = Result<(), Error>;
}

impl Handler<CreateWallet> for Storage {
    type Result = <CreateWallet as Message>::Result;

    fn handle(
        &mut self,
        CreateWallet(db, wallet, password): CreateWallet,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.create_wallet(db.as_ref(), wallet, password)
    }
}
