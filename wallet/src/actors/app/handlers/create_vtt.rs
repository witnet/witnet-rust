use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::CreateVttRequest {
    type Result = Result<api::CreateVttResponse, api::Error>;
}

impl Handler<api::CreateVttRequest> for App {
    type Result = Result<api::CreateVttResponse, api::Error>;

    fn handle(&mut self, _msg: api::CreateVttRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(api::CreateVttResponse {
            transaction_id: "389a3fa3a1feb8fd8cdc61748ac17dce0aeef39ff9634dec9c20ece69105c264"
                .to_string(),
        })
    }
}
