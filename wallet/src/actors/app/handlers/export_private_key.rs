use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{actors::app, types};

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportPrivateKeyRequest {
    pub wallet_id: String,
    pub password: types::Password,
    pub session_id: types::SessionId,
}

#[derive(Serialize)]
pub struct ExportPrivateKeyResponse {
    private_key: String,
}

impl Message for ExportPrivateKeyRequest {
    type Result = Result<ExportPrivateKeyResponse, app::Error>;
}

impl Handler<ExportPrivateKeyRequest> for app::App {
    type Result = app::ResponseActFuture<ExportPrivateKeyResponse>;

    fn handle(&mut self, msg: ExportPrivateKeyRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self
            .export_private_key(msg.session_id, msg.wallet_id, msg.password)
            .map(|private_key, _, _| ExportPrivateKeyResponse { private_key });

        Box::new(f)
    }
}