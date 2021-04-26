use actix::prelude::*;
use serde::{Deserialize, Serialize};
use witnet_futures_utils::ActorFutureExt2;

use crate::{actors::app, types};

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateWalletRequest {
    session_id: types::SessionId,
    wallet_id: String,
    name: Option<String>,
    description: Option<String>,
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
            .update_wallet(req.session_id, req.wallet_id, req.name, req.description)
            .map_ok(|(), _, _| UpdateWalletResponse { success: true });

        Box::pin(f)
    }
}
