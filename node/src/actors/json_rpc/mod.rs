mod connection;
/// JSON-RPC methods
pub mod json_rpc_methods;
mod newline_codec;
mod server;

pub use self::server::JsonRpcServer;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Subscriptions. Indexed by method_name, then subscription id
pub type Subscriptions = Arc<
    Mutex<
        HashMap<
            &'static str,
            HashMap<jsonrpc_pubsub::SubscriptionId, (jsonrpc_pubsub::Sink, jsonrpc_core::Value)>,
        >,
    >,
>;

/// Notification sent as a result to a subscription
pub struct SubscriptionResult {
    /// Result of the subscription
    pub result: jsonrpc_core::Value,
    /// Subscription id
    pub subscription: jsonrpc_pubsub::SubscriptionId,
}

impl From<SubscriptionResult> for jsonrpc_core::Params {
    fn from(x: SubscriptionResult) -> Self {
        let mut map = serde_json::Map::new();
        map.insert("result".to_string(), x.result);
        map.insert(
            "subscription".to_string(),
            match x.subscription {
                jsonrpc_pubsub::SubscriptionId::Number(x) => {
                    serde_json::Value::Number(serde_json::Number::from(x))
                }
                jsonrpc_pubsub::SubscriptionId::String(s) => serde_json::Value::String(s),
            },
        );
        jsonrpc_core::Params::Map(map)
    }
}
