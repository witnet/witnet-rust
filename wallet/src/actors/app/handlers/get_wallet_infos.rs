//! TODO: doc
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct GetWalletInfos;

impl GetWalletInfos {}

impl Message for GetWalletInfos {
    type Result = ();
}

impl Handler<GetWalletInfos> for App {
    type Result = ();

    fn handle(&mut self, _msg: GetWalletInfos, _ctx: &mut Self::Context) -> Self::Result {}
}
