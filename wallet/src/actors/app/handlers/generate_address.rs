//! TODO: doc
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct GenerateAddress(pub ());

impl GenerateAddress {}

impl Message for GenerateAddress {
    type Result = ();
}

impl Handler<GenerateAddress> for App {
    type Result = ();

    fn handle(&mut self, _msg: GenerateAddress, _ctx: &mut Self::Context) -> Self::Result {}
}
