use jsonrpc_pubsub as pubsub;
use serde::Serialize;

pub struct SubscribeRequest(pub pubsub::Subscriber);

#[derive(Debug, Serialize)]
pub struct SubscribeResponse {
    #[serde(rename = "subscriptionId")]
    pub id: usize,
}
