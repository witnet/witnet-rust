//! # Application actor.
//!
//! See [`App`](App) actor for more information.
use std::path::PathBuf;

use actix::prelude::*;
use failure::Error;
use futures::future;
use jsonrpc_core as rpc;
use jsonrpc_pubsub as pubsub;
use serde_json::{self as json, json};

use super::{
    rad_executor::RadExecutor,
    storage::{self, Storage},
};
use witnet_net::client::tcp::{jsonrpc as rpc_client, JsonRpcClient};

pub mod error;
mod handlers;

/// Application actor.
///
/// The application actor is in charge of managing the state of the application and coordinating the
/// service actors, e.g.: storage, node client, and so on.
pub struct App {
    storage: Addr<Storage>,
    rad_executor: Addr<RadExecutor>,
    node_client: Option<Addr<JsonRpcClient>>,
    subscriptions: [Option<pubsub::Sink>; 10],
}

impl App {
    pub fn build() -> AppBuilder {
        AppBuilder::default()
    }

    /// Return an id for a new subscription. If there are no available subscription slots, then
    /// `None` is returned.
    pub fn subscribe(&mut self, subscriber: pubsub::Subscriber) -> Result<usize, error::Error> {
        let (id, slot) = self
            .subscriptions
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.is_none())
            .ok_or_else(|| error::Error::SubscribeFailed("max limit of subscriptions reached"))?;

        *slot = subscriber
            .assign_id(pubsub::SubscriptionId::from(id as u64))
            .ok();

        Ok(id)
    }

    /// Remove a subscription and leave its corresponding slot free.
    pub fn unsubscribe(&mut self, id: pubsub::SubscriptionId) -> Result<(), error::Error> {
        let index = match id {
            pubsub::SubscriptionId::Number(n) => Ok(n as usize),
            _ => Err(error::Error::UnsubscribeFailed(
                "subscription id must be a number",
            )),
        }?;
        let slot = self
            .subscriptions
            .as_mut()
            .get_mut(index)
            .ok_or_else(|| error::Error::UnsubscribeFailed("subscription id not found"))?;

        *slot = None;

        Ok(())
    }

    /// Forward a Json-RPC call to the node.
    pub fn forward(
        &mut self,
        method: String,
        params: rpc::Params,
    ) -> ResponseFuture<json::Value, error::Error> {
        match &self.node_client {
            Some(addr) => {
                let req = rpc_client::Request::method(method)
                    .params(params)
                    .expect("rpc::Params failed serialization");
                let fut = addr
                    .send(req)
                    .map_err(error::Error::RequestFailedToSend)
                    .and_then(|result| result.map_err(error::Error::RequestFailed));

                Box::new(fut)
            }
            None => {
                let fut = future::err(error::Error::NodeNotConnected);

                Box::new(fut)
            }
        }
    }
}

#[derive(Default)]
pub struct AppBuilder {
    node_url: Option<String>,
    db_path: PathBuf,
}

impl AppBuilder {
    pub fn node_url(mut self, url: Option<String>) -> Self {
        self.node_url = url;
        self
    }

    pub fn db_path(mut self, path: PathBuf) -> Self {
        self.db_path = path;
        self
    }

    /// Start App actor with given addresses for Storage and Rad actors.
    pub fn start(self) -> Result<Addr<App>, Error> {
        let node_url = self.node_url;
        let node_client = node_url.clone().map_or_else(
            || Ok(None),
            |url| JsonRpcClient::start(url.as_ref()).map(Some),
        )?;
        let storage = Storage::build()
            .with_path(self.db_path)
            .with_file_name("witnet_wallets.db")
            .with_options({
                let mut db_opts = storage::Options::default();
                db_opts.create_if_missing(true);
                db_opts
            })
            .start()?;
        let rad_executor = RadExecutor::start();

        let app = App {
            storage,
            rad_executor,
            node_client,
            subscriptions: Default::default(),
        };

        Ok(app.start())
    }
}

impl Actor for App {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // controller::Controller::from_registry()
        //     .do_send(controller::Subscribe(ctx.address().recipient()));
        if let Some(ref client) = self.node_client {
            let recipient = ctx.address().recipient();
            let request =
                rpc_client::Request::method("witnet_subscribe").value(json!(["newBlocks"]));
            client.do_send(rpc_client::SetSubscriber(recipient, request));
        }
    }
}

impl Supervised for App {}

fn json_get<'a>(value: &'a json::Value, path: &[&str]) -> Option<&'a json::Value> {
    let mut result = Some(value);
    for key in path {
        result = result.and_then(|v| v.get(key));
    }
    result
}

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
                let mut obj = json::Map::new();
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
