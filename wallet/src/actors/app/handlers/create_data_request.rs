//! TODO: doc
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct CreateDataRequest(pub ());

impl CreateDataRequest {}

impl Message for CreateDataRequest {
    type Result = ();
}

impl Handler<CreateDataRequest> for App {
    type Result = ();

    fn handle(&mut self, _msg: CreateDataRequest, _ctx: &mut Self::Context) -> Self::Result {}
}
