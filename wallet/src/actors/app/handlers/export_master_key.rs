use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{actors::app, types};

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportMasterKeyRequest {
    pub wallet_id: String,
    pub password: types::Password,
    pub session_id: types::SessionId,
}

#[derive(Serialize)]
pub struct ExportMasterKeyResponse {
    master_key: String,
}

impl Message for ExportMasterKeyRequest {
    type Result = Result<ExportMasterKeyResponse, app::Error>;
}

impl Handler<ExportMasterKeyRequest> for app::App {
    type Result = app::ResponseActFuture<ExportMasterKeyResponse>;

    fn handle(&mut self, msg: ExportMasterKeyRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self
            .export_master_key(msg.session_id, msg.wallet_id, msg.password)
            .map(|master_key, _, _| ExportMasterKeyResponse { master_key });

        Box::new(f)
    }
}
