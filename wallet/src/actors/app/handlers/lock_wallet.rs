//! TODO: doc
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct LockWallet(pub ());

impl LockWallet {}

impl Message for LockWallet {
    type Result = ();
}

impl Handler<LockWallet> for App {
    type Result = ();

    fn handle(&mut self, _msg: LockWallet, _ctx: &mut Self::Context) -> Self::Result {}
}
