//! TODO: doc
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;
use crate::error;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct ImportSeed(pub ());

impl ImportSeed {}

impl Message for ImportSeed {
    type Result = Result<(), error::Error>;
}

impl Handler<ImportSeed> for App {
    type Result = Result<(), error::Error>;

    fn handle(&mut self, _msg: ImportSeed, _ctx: &mut Self::Context) -> Self::Result {
        unimplemented!()
    }
}
