use actix::prelude::*;

use crate::actors::Crypto;
use crate::{app, crypto, wallet};

pub struct GenWalletKeys(pub app::SeedSource);

impl Message for GenWalletKeys {
    type Result = Result<(String, wallet::MasterKey), crypto::Error>;
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
