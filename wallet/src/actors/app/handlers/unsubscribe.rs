use actix::prelude::*;

use crate::actors::App;
use crate::{api, app};

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
        self.unsubscribe(&id)
            .map(|_| log::debug!("Subscription {:?} removed", id))
            .map_err(|err| match err {
                app::Error::UnknownSession => api::Error::Unauthorized,
                err => api::internal_error(err),
            })
    }
}
