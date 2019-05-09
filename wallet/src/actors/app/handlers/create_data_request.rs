//! TODO: doc
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;
use crate::error;

/// TODO: doc
#[derive(Debug, Deserialize)]
pub struct CreateDataRequest(pub ());

impl CreateDataRequest {}

impl Message for CreateDataRequest {
    type Result = Result<(), error::Error>;
}

impl Handler<CreateDataRequest> for App {
    type Result = Result<(), error::Error>;

    fn handle(&mut self, _msg: CreateDataRequest, _ctx: &mut Self::Context) -> Self::Result {
        unimplemented!()
    }
}
