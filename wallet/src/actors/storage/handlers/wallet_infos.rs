use actix::prelude::*;

use crate::actors::storage::Storage;
use crate::{storage::Error, wallet};

/// Get the list of created wallets along with their ids
pub struct GetWalletInfos;

impl Message for GetWalletInfos {
    type Result = Result<Vec<wallet::WalletInfo>, Error>;
}

impl Handler<GetWalletInfos> for Storage {
    type Result = Result<Vec<wallet::WalletInfo>, Error>;

    fn handle(&mut self, _msg: GetWalletInfos, _ctx: &mut Self::Context) -> Self::Result {
        self.get_wallet_infos()
    }
}
