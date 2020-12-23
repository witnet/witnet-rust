use actix::prelude::*;

use crate::actors::app;

pub struct UnsubscribeRequest(pub jsonrpc_pubsub::SubscriptionId);

impl Message for UnsubscribeRequest {
    type Result = app::Result<()>;
}

impl Handler<UnsubscribeRequest> for app::App {
    type Result = <UnsubscribeRequest as Message>::Result;

    fn handle(
        &mut self,
        UnsubscribeRequest(id): UnsubscribeRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.unsubscribe(&id)
            .map(|()| log::debug!("Subscription {:?} removed", id))
    }
}
