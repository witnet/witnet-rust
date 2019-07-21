use actix::prelude::*;

use crate::actors::app;
use crate::types;

pub struct UnsubscribeRequest(pub types::SubscriptionId);

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
