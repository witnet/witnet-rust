//! TODO: doc
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;
use crate::error;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct UnlockWallet(pub ());

impl UnlockWallet {}

impl Message for UnlockWallet {
    type Result = Result<(), error::Error>;
}

impl Handler<UnlockWallet> for App {
    type Result = Result<(), error::Error>;

    fn handle(&mut self, _msg: UnlockWallet, _ctx: &mut Self::Context) -> Self::Result {
        unimplemented!()
    }
}
