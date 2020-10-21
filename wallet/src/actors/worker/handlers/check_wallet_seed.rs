use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct CheckWalletSeedRequest(
    /// Wallet seed source (mnemonics or xpriv)
    pub types::SeedSource,
);

impl Message for CheckWalletSeedRequest {
    type Result = worker::Result<(bool, String)>;
}

impl Handler<CheckWalletSeedRequest> for worker::Worker {
    type Result = <CheckWalletSeedRequest as Message>::Result;

    fn handle(
        &mut self,
        CheckWalletSeedRequest(seed): CheckWalletSeedRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.check_wallet_seed(seed)
    }
}
