use actix::prelude::*;

use crate::actors::worker;
use crate::types;

/// Execute the containing RAD-request.
pub struct RunRadRequest(pub types::RADRequest);

impl Message for RunRadRequest {
    type Result = worker::Result<types::RadonTypes>;
}

impl Handler<RunRadRequest> for worker::Worker {
    type Result = <RunRadRequest as Message>::Result;

    fn handle(
        &mut self,
        RunRadRequest(request): RunRadRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        log::debug!("Executing RAD request");
        self.run_rad_request(request)
    }
}
