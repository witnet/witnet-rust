use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{actors::app, model, types};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct UtxoInfoRequest {
    session_id: types::SessionId,
    wallet_id: String,
}

pub type UtxoInfoResponse = HashMap<String, model::OutputInfo>;

impl Message for UtxoInfoRequest {
    type Result = app::Result<UtxoInfoResponse>;
}

impl Handler<UtxoInfoRequest> for app::App {
    type Result = app::ResponseActFuture<UtxoInfoResponse>;

    fn handle(&mut self, msg: UtxoInfoRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self
            .get_utxo_info(msg.session_id, msg.wallet_id)
            .map_ok(|utxo_set, _act, _ctx| {
                utxo_set
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect()
            });

        Box::pin(f)
    }
}
