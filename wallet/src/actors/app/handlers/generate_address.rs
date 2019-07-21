use actix::prelude::*;
use serde::Deserialize;

use crate::actors::app;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateAddressRequest {
    session_id: String,
    wallet_id: String,
    label: Option<String>,
}

pub struct GenerateAddressResponse {
    address: String,
    path: String,
}

impl Message for GenerateAddressRequest {
    type Result = app::Result<GenerateAddressResponse>;
}

impl Handler<GenerateAddressRequest> for app::App {
    type Result = app::ResponseActFuture<GenerateAddressResponse>;

    fn handle(&mut self, msg: GenerateAddressRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self
            .generate_address(msg.session_id, msg.wallet_id, msg.label)
            .map(|key, _, _| GenerateAddressResponse {
                address: key.address(),
                path: key.path,
            });

        Box::new(f)
    }
}
