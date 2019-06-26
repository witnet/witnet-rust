use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::SubscribeRequest {
    type Result = Result<api::SubscribeResponse, api::Error>;
}

impl Handler<api::SubscribeRequest> for App {
    type Result = Result<api::SubscribeResponse, api::Error>;

    fn handle(
        &mut self,
        api::SubscribeRequest(subscriber): api::SubscribeRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.subscribe(subscriber)
            .map_err(api::internal_error)
            .map(|id| api::SubscribeResponse { id })
    }
}
