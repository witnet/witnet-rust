use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;

#[derive(Debug, Serialize, Deserialize)]
pub struct RunRadReqRequest {
    pub rad_request: types::RADRequest,
}

#[derive(Debug, Serialize)]
pub struct RunRadReqResponse {
    pub result: types::RadonReport<types::RadonTypes>,
}

impl Message for RunRadReqRequest {
    type Result = app::Result<RunRadReqResponse>;
}

impl Handler<RunRadReqRequest> for app::App {
    type Result = app::ResponseFuture<RunRadReqResponse>;

    fn handle(&mut self, msg: RunRadReqRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self
            .run_rad_request(msg.rad_request)
            .map_err(app::internal_error)
            .map(|result| RunRadReqResponse { result });

        Box::new(f)
    }
}
