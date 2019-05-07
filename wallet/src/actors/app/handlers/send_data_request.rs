//! TODO: doc
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct SendDataRequest(pub ());

impl SendDataRequest {}

impl Message for SendDataRequest {
    type Result = ();
}

impl Handler<SendDataRequest> for App {
    type Result = ();

    fn handle(&mut self, _msg: SendDataRequest, _ctx: &mut Self::Context) -> Self::Result {}
}
