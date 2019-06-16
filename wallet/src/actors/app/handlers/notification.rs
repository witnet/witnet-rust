use actix::prelude::*;

use jsonrpc_core as rpc;
use jsonrpc_pubsub as pubsub;
use serde_json::{Map, Value};

use witnet_net::client::tcp::jsonrpc as rpc_client;

use crate::actors::App;

impl Handler<rpc_client::Notification> for App {
    type Result = <rpc_client::Notification as Message>::Result;

    fn handle(&mut self, msg: rpc_client::Notification, ctx: &mut Self::Context) -> Self::Result {
        let checkpoint = json_get(&msg.0, &["block_header", "beacon", "checkpoint"]);
        log::debug!(
            ">> Received notification from jsonrpc-client with checkpoint: {:?}",
            checkpoint
        );
        self.subscriptions
            .iter()
            .filter_map(|s| s.as_ref())
            .enumerate()
            .for_each(|(slot, subscriber)| {
                let value = msg.0.clone();
                let mut obj = Map::new();
                obj.insert("newBlock".to_string(), value);

                let params = rpc::Params::Map(obj);

                log::debug!("Sending notification to wallet-subscribers.");

                subscriber
                    .notify(params)
                    .map(|_| ())
                    .into_actor(self)
                    .map_err(move |err, act, _ctx| {
                        let id = pubsub::SubscriptionId::Number(slot as u64);
                        act.unsubscribe(id)
                            .expect("failed to removed faulty subscription");
                        log::error!("Error notifying client: {}.", err,);
                    })
                    .spawn(ctx);
            });
    }
}

fn json_get<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut result = Some(value);
    for key in path {
        result = result.and_then(|v| v.get(key));
    }
    result
}
