use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDataReqRequest {
    pub rad_request: types::RADRequest,
}

impl Message for CreateDataReqRequest {
    type Result = app::Result<()>;
}

impl Handler<CreateDataReqRequest> for app::App {
    type Result = <CreateDataReqRequest as Message>::Result;

    fn handle(&mut self, _msg: CreateDataReqRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
