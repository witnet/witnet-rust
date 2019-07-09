use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::UnsubscribeRequest {
    type Result = Result<(), api::Error>;
}

impl Handler<api::UnsubscribeRequest> for App {
    type Result = Result<(), api::Error>;

    fn handle(
        &mut self,
        api::UnsubscribeRequest(id): api::UnsubscribeRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        Ok(self.unsubscribe(id))
    }
}
