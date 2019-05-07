//! TODO: doc
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct ImportSeed(pub ());

impl ImportSeed {}

impl Message for ImportSeed {
    type Result = ();
}

impl Handler<ImportSeed> for App {
    type Result = ();

    fn handle(&mut self, _msg: ImportSeed, _ctx: &mut Self::Context) -> Self::Result {}
}
