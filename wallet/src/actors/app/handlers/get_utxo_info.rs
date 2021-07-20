use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{actors::app, model, types};

#[derive(Debug, Serialize, Deserialize)]
pub struct UtxoInfoRequest {
    session_id: types::SessionId,
    wallet_id: String,
}

pub type UtxoInfoResponse = model::UtxoSet;

impl Message for UtxoInfoRequest {
    type Result = app::Result<UtxoInfoResponse>;
}

impl Handler<UtxoInfoRequest> for app::App {
    type Result = app::ResponseActFuture<UtxoInfoResponse>;

    fn handle(&mut self, msg: UtxoInfoRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self.get_utxo_info(msg.session_id, msg.wallet_id);

        Box::pin(f)
    }
}
