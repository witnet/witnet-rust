use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::NextSubscriptionId {
    type Result = Result<u64, api::Error>;
}

impl Handler<api::NextSubscriptionId> for App {
    type Result = Result<u64, api::Error>;

    fn handle(&mut self, _msg: api::NextSubscriptionId, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self.next_subscription_id())
    }
}
