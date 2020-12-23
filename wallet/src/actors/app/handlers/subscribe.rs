use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;

#[derive(Serialize, Deserialize)]
pub struct SubscribeRequest {
    pub session_id: types::SessionId,
}

pub struct Subscribe(
    pub types::SessionId,
    pub jsonrpc_pubsub::SubscriptionId,
    pub jsonrpc_pubsub::Sink,
);

impl Message for Subscribe {
    type Result = app::Result<()>;
}

impl Handler<Subscribe> for app::App {
    type Result = <Subscribe as Message>::Result;

    fn handle(
        &mut self,
        Subscribe(session_id, subscription_id, sink): Subscribe,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.subscribe(session_id.clone(), subscription_id, sink)
            .map(|()| log::debug!("Created subscription for session: {}", session_id))
            .map_err(|err| {
                log::error!(
                    "Couldn't create subscription for session {}: {}",
                    session_id,
                    err
                );
                err
            })
    }
}
