use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;

#[derive(Debug, Serialize, Deserialize)]
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
