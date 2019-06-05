use actix::prelude::*;

use crate::actors::App;
use crate::api;
use crate::error;

impl Message for api::UnsubscribeRequest {
    type Result = Result<api::UnsubscribeResponse, error::Error>;
}

impl Handler<api::UnsubscribeRequest> for App {
    type Result = Result<api::UnsubscribeResponse, error::Error>;

    fn handle(
        &mut self,
        api::UnsubscribeRequest(id): api::UnsubscribeRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.unsubscribe(id).map_err(error::Error::Subscription)
    }
}
