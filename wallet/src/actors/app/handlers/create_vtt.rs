use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;

#[derive(Debug, Deserialize)]
pub struct CreateVttRequest {
    address: String,
    label: String,
    amount: u64,
    fee: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVttResponse {
    pub transaction_id: String,
}

impl Message for CreateVttRequest {
    type Result = app::Result<CreateVttResponse>;
}

impl Handler<CreateVttRequest> for app::App {
    type Result = <CreateVttRequest as Message>::Result;

    fn handle(&mut self, _msg: CreateVttRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(CreateVttResponse {
            transaction_id: "389a3fa3a1feb8fd8cdc61748ac17dce0aeef39ff9634dec9c20ece69105c264"
                .to_string(),
        })
    }
}
