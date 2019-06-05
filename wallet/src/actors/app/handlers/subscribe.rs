use actix::prelude::*;

use crate::actors::App;
use crate::api;
use crate::error;

impl Message for api::SubscribeRequest {
    type Result = Result<api::SubscribeResponse, error::Error>;
}

impl Handler<api::SubscribeRequest> for App {
    type Result = Result<api::SubscribeResponse, error::Error>;

    fn handle(
        &mut self,
        api::SubscribeRequest(subscriber): api::SubscribeRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.subscribe(subscriber)
            .map(|id| api::SubscribeResponse { id })
            .map_err(error::Error::Subscription)
    }
}
