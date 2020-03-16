use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::{model, types};

/// Request to sign strings after being hashed with SHA256.
#[derive(Debug, Serialize, Deserialize)]
pub struct SignDataRequest {
    session_id: types::SessionId,
    wallet_id: String,
    // Message to be signed
    data: String,
}

pub type SignDataResponse = model::ExtendedKeyedSignature;

impl Message for SignDataRequest {
    type Result = app::Result<SignDataResponse>;
}

impl Handler<SignDataRequest> for app::App {
    type Result = app::ResponseActFuture<SignDataResponse>;

    fn handle(&mut self, msg: SignDataRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self.sign_data(&msg.session_id, &msg.wallet_id, msg.data);

        Box::new(f)
    }
}
