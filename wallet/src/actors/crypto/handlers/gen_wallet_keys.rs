use actix::prelude::*;

use crate::actors::{crypto::Error, Crypto};
use crate::wallet;

pub struct GenWalletKeys(pub wallet::SeedSource);

impl Message for GenWalletKeys {
    type Result = Result<(String, wallet::MasterKey), Error>;
}

impl Handler<GenWalletKeys> for Crypto {
    type Result = <GenWalletKeys as Message>::Result;

    fn handle(
        &mut self,
        GenWalletKeys(seed_source): GenWalletKeys,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let master = self.gen_master_key(seed_source)?;
        let id = self.gen_id(&master);

        Ok((id, master))
    }
}
