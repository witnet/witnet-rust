use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct Set(
    pub types::SessionWallet,
    /// Key
    pub String,
    /// Value
    pub String,
);

impl Message for Set {
    type Result = worker::Result<()>;
}

impl Handler<Set> for worker::Worker {
    type Result = <Set as Message>::Result;

    fn handle(&mut self, Set(wallet, key, value): Set, _ctx: &mut Self::Context) -> Self::Result {
        self.set(&wallet, &key, &value)
    }
}
