use jsonrpc_pubsub as pubsub;
use serde::Deserialize;

use crate::app;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeRequest {
    pub(crate) session_id: app::SessionId,
}

pub struct Subscribe(pub(crate) SubscribeRequest, pub(crate) pubsub::Sink);

/// Internal message to obtain a new subscription id.
pub struct NextSubscriptionId;
