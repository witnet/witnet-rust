use std::collections::HashMap;

use actix::prelude::*;
use serde_json::json;

use witnet_net::client::tcp::jsonrpc;

use crate::{model, types};

pub mod error;
pub mod handlers;
pub mod methods;
pub mod params;
pub mod routes;
pub mod validation;

pub use error::*;
pub use handlers::*;
pub use params::*;
pub use routes::*;
pub use validation::*;

pub type Result<T> = std::result::Result<T, Error>;

pub type ResponseFuture<T> = actix::ResponseFuture<T, Error>;

pub type ResponseActFuture<T> = actix::ResponseActFuture<App, T, Error>;

pub struct App {
    params: Params,
    sessions: HashMap<String, Session>,
}

#[derive(Default)]
struct Session {
    wallets: HashMap<String, model::WalletUnlocked>,
    subscriptions: HashMap<types::SubscriptionId, types::Sink>,
}

impl Actor for App {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // Subscribe to node if there's one configured.
        if let Some(ref client) = self.params.client {
            let recipient = ctx.address().recipient();
            let request = types::RpcRequest::method("witnet_subscribe")
                .timeout(self.params.requests_timeout)
                .value(json!(["newBlocks"]));

            client.do_send(jsonrpc::SetSubscriber(recipient, request));
        }
    }
}
