//! TODO: doc
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;
use crate::error;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct CreateWallet(pub ());

impl CreateWallet {}

impl Message for CreateWallet {
    type Result = Result<(), error::Error>;
}

impl Handler<CreateWallet> for App {
    type Result = Result<(), error::Error>;

    fn handle(&mut self, _msg: CreateWallet, _ctx: &mut Self::Context) -> Self::Result {
        unimplemented!()
    }
}
