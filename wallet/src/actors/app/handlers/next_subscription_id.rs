use actix::prelude::*;

use crate::actors::App;
use crate::{api, app, types};

impl Message for api::NextSubscriptionId {
    type Result = Result<types::SubscriptionId, api::Error>;
}

impl Handler<api::NextSubscriptionId> for App {
    type Result = Result<types::SubscriptionId, api::Error>;

    fn handle(
        &mut self,
        api::NextSubscriptionId(session_id): api::NextSubscriptionId,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.next_subscription_id(session_id)
            .map_err(|err| match err {
                app::Error::UnknownSession => api::Error::Unauthorized,
                err => api::internal_error(err),
            })
    }
}
