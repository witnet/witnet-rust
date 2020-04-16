use actix::prelude::*;

use crate::actors::app;
use crate::types;

pub struct NextSubscriptionId(pub types::SessionId);

impl Message for NextSubscriptionId {
    type Result = app::Result<types::SubscriptionId>;
}

impl Handler<NextSubscriptionId> for app::App {
    type Result = <NextSubscriptionId as Message>::Result;

    fn handle(
        &mut self,
        NextSubscriptionId(session_id): NextSubscriptionId,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.next_subscription_id(&session_id)
    }
}
