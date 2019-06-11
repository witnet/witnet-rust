use actix::prelude::*;
use bincode::{deserialize, serialize};

use crate::actors::storage::{error::Error, Storage};
use crate::wallet;

/// Get the list of created wallets along with their ids
pub struct GetWalletInfos;

impl Message for GetWalletInfos {
    type Result = Result<Vec<wallet::WalletInfo>, Error>;
}

impl Handler<GetWalletInfos> for Storage {
    type Result = Result<Vec<wallet::WalletInfo>, Error>;

    fn handle(&mut self, _msg: GetWalletInfos, _ctx: &mut Self::Context) -> Self::Result {
        let key = serialize("wallet-infos").map_err(Error::Serialization)?;
        let result = self.db.get(key).map_err(Error::Db)?;

        match result {
            Some(db_vec) => {
                let value = deserialize(db_vec.as_ref()).map_err(Error::Serialization)?;
                Ok(value)
            }
            None => Ok(Vec::new()),
        }
    }
}
