//! # Handler for subscribe messages
//!
//! Used if a client wants to subscribe to realtime updates in the wallet.
//! See `Subscribe` struct for more info.
use actix::prelude::*;
use jsonrpc_pubsub as pubsub;

use crate::actors::App;
use crate::error;

/// Subscribe message. It will store the subscriber in an internal buffer and on every update it
/// will be notified.
pub struct Subscribe(pub pubsub::Subscriber);

impl Message for Subscribe {
    type Result = Result<pubsub::SubscriptionId, error::Error>;
}

impl Handler<Subscribe> for App {
    type Result = Result<pubsub::SubscriptionId, error::Error>;

    fn handle(
        &mut self,
        Subscribe(subscriber): Subscribe,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.subscribe(subscriber)
            .map_err(error::Error::Subscription)
    }
}
