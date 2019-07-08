use actix::prelude::*;

use crate::actors::storage::Storage;
use crate::{storage::Error, types};

/// Get the list of created wallets along with their ids
pub struct GetWalletInfos(pub types::SharedDB);

impl Message for GetWalletInfos {
    type Result = Result<Vec<types::WalletInfo>, Error>;
}

impl Handler<GetWalletInfos> for Storage {
    type Result = Result<Vec<types::WalletInfo>, Error>;

    fn handle(
        &mut self,
        GetWalletInfos(db): GetWalletInfos,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.get_wallet_infos(db.as_ref())
    }
}
