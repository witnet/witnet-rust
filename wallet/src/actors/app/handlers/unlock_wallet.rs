//! TODO: doc
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct UnlockWallet(pub ());

impl UnlockWallet {}

impl Message for UnlockWallet {
    type Result = ();
}

impl Handler<UnlockWallet> for App {
    type Result = ();

    fn handle(&mut self, _msg: UnlockWallet, _ctx: &mut Self::Context) -> Self::Result {}
}
