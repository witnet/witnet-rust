use actix::prelude::*;
use serde::{Deserialize, Serialize};
use witnet_futures_utils::ActorFutureExt;

use crate::actors::app;
use crate::{model, types};

#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateAddressRequest {
    session_id: types::SessionId,
    wallet_id: String,
    external: Option<bool>,
    label: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GenerateAddressResponse {
    pub address: String,
    pub path: String,
}

impl Message for GenerateAddressRequest {
    type Result = app::Result<GenerateAddressResponse>;
}

impl Handler<GenerateAddressRequest> for app::App {
    type Result = app::ResponseActFuture<GenerateAddressResponse>;

    fn handle(&mut self, msg: GenerateAddressRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self
            .generate_address(
                msg.session_id,
                msg.wallet_id,
                msg.external.unwrap_or(true),
                msg.label,
            )
            .map_ok(
                |model::Address { address, path, .. }, _, _| GenerateAddressResponse {
                    address,
                    path,
                },
            );

        Box::pin(f)
    }
}
