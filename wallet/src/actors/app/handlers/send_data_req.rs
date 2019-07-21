use actix::prelude::*;
use serde::Deserialize;

use crate::actors::app;

#[derive(Debug, Deserialize)]
pub struct SendDataReqRequest;

impl Message for SendDataReqRequest {
    type Result = app::Result<()>;
}

impl Handler<SendDataReqRequest> for app::App {
    type Result = <SendDataReqRequest as Message>::Result;

    fn handle(&mut self, _msg: SendDataReqRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
