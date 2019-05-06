//! TODO: doc
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct RunDataRequest(pub ());

impl RunDataRequest {}

impl Message for RunDataRequest {
    type Result = ();
}

impl Handler<RunDataRequest> for App {
    type Result = ();

    fn handle(&mut self, _msg: RunDataRequest, _ctx: &mut Self::Context) -> Self::Result {}
}
