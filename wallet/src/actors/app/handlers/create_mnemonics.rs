//! TODO: doc
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct CreateMnemonics(pub ());

impl CreateMnemonics {}

impl Message for CreateMnemonics {
    type Result = ();
}

impl Handler<CreateMnemonics> for App {
    type Result = ();

    fn handle(&mut self, _msg: CreateMnemonics, _ctx: &mut Self::Context) -> Self::Result {}
}
