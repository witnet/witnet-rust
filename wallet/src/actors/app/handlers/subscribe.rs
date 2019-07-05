use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::Subscribe {
    type Result = Result<(), api::Error>;
}

impl Handler<api::Subscribe> for App {
    type Result = Result<(), api::Error>;

    fn handle(
        &mut self,
        api::Subscribe(request, subscriber): api::Subscribe,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.subscribe(subscriber)
            .map_err(api::internal_error)
            .map(|_| ())
    }
}
