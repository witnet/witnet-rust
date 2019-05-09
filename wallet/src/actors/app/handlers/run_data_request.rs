//! TODO: doc
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;
use crate::error;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct RunDataRequest(pub ());

impl RunDataRequest {}

impl Message for RunDataRequest {
    type Result = Result<(), error::Error>;
}

impl Handler<RunDataRequest> for App {
    type Result = Result<(), error::Error>;

    fn handle(&mut self, _msg: RunDataRequest, _ctx: &mut Self::Context) -> Self::Result {
        unimplemented!()
    }
}
