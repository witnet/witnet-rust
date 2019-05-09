//! TODO: doc
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;
use crate::error;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct SendDataRequest(pub ());

impl SendDataRequest {}

impl Message for SendDataRequest {
    type Result = Result<(), error::Error>;
}

impl Handler<SendDataRequest> for App {
    type Result = Result<(), error::Error>;

    fn handle(&mut self, _msg: SendDataRequest, _ctx: &mut Self::Context) -> Self::Result {
        unimplemented!()
    }
}
