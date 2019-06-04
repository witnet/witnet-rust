//! # Handler for the unsubscribe methods
//!
//! Used by the client to stop receiving realtime updates from the wallet.
//! See `Unsubscribe` struct for more info.
use actix::prelude::*;
use jsonrpc_pubsub as pubsub;

use crate::actors::App;
use crate::error;

/// Unsubscribe message. It will remove the specified subscription from the internal list of
/// subscriptions.
pub struct Unsubscribe(pub pubsub::SubscriptionId);

impl Message for Unsubscribe {
    type Result = Result<(), error::Error>;
}

impl Handler<Unsubscribe> for App {
    type Result = Result<(), error::Error>;

    fn handle(&mut self, Unsubscribe(id): Unsubscribe, _ctx: &mut Self::Context) -> Self::Result {
        self.unsubscribe(id).map_err(error::Error::Subscription)
    }
}
