use actix::prelude::*;

use crate::actors::worker;
use crate::model;

pub struct Get(
    pub model::WalletUnlocked,
    /// Key
    pub String,
);

impl Message for Get {
    type Result = worker::Result<Option<String>>;
}

impl Handler<Get> for worker::Worker {
    type Result = <Get as Message>::Result;

    fn handle(&mut self, Get(wallet, key): Get, _ctx: &mut Self::Context) -> Self::Result {
        self.get(&wallet, &key)
    }
}
