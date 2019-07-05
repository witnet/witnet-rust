use jsonrpc_pubsub as pubsub;
use serde::Deserialize;

use crate::types;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeRequest {
    pub(crate) session_id: types::SessionId,
}

pub struct Subscribe(
    pub types::SessionId,
    pub types::SubscriptionId,
    pub pubsub::Sink,
);

pub struct NextSubscriptionId(pub types::SessionId);
