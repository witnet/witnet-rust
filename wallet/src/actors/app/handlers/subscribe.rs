use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::Subscribe {
    type Result = Result<(), api::Error>;
}

impl Handler<api::Subscribe> for App {
    type Result = <api::Subscribe as Message>::Result;

    fn handle(
        &mut self,
        api::Subscribe(session_id, subscription_id, sink): api::Subscribe,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        match self.subscribe(session_id.clone(), subscription_id, sink) {
            Ok(()) => {
                log::debug!("Created subscription for session: {}", session_id);
                Ok(())
            }
            Err(err) => {
                log::error!(
                    "Couldn't create subscription for session {}: {}",
                    session_id,
                    err
                );
                Err(api::internal_error(err))
            }
        }
    }
}
