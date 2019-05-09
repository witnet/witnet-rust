//! TODO: doc
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;
use crate::error;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct CreateMnemonics(pub ());

impl CreateMnemonics {}

impl Message for CreateMnemonics {
    type Result = Result<(), error::Error>;
}

impl Handler<CreateMnemonics> for App {
    type Result = Result<(), error::Error>;

    fn handle(&mut self, _msg: CreateMnemonics, _ctx: &mut Self::Context) -> Self::Result {
        unimplemented!()
    }
}
