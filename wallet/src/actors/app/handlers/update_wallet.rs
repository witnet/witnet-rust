use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateWalletRequest {
    session_id: types::SessionId,
    wallet_id: String,
    name: Option<String>,
    caption: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateWalletResponse {
    pub success: bool,
}

impl Message for UpdateWalletRequest {
    type Result = app::Result<UpdateWalletResponse>;
}

impl Handler<UpdateWalletRequest> for app::App {
    type Result = app::ResponseActFuture<UpdateWalletResponse>;

    fn handle(&mut self, req: UpdateWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self
            .update_wallet(req.session_id, req.wallet_id, req.name, req.caption)
            .map(|(), _, _| UpdateWalletResponse { success: true });

        Box::new(f)
    }
}
