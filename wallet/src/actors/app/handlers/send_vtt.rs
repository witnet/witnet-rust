//! TODO: doc
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;
use crate::error;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct SendVtt(pub ());

impl SendVtt {}

impl Message for SendVtt {
    type Result = Result<(), error::Error>;
}

impl Handler<SendVtt> for App {
    type Result = Result<(), error::Error>;

    fn handle(&mut self, _msg: SendVtt, _ctx: &mut Self::Context) -> Self::Result {
        unimplemented!()
    }
}
