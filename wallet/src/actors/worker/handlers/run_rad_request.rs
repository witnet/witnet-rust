use actix::prelude::*;

use crate::actors::worker;
use witnet_data_structures::chain::RADRequest;
use witnet_rad::RADRequestExecutionReport;

/// Execute the containing RAD-request.
pub struct RunRadRequest {
    pub request: RADRequest,
}

impl Message for RunRadRequest {
    type Result = worker::Result<RADRequestExecutionReport>;
}

impl Handler<RunRadRequest> for worker::Worker {
    type Result = <RunRadRequest as Message>::Result;

    fn handle(
        &mut self,
        RunRadRequest { request }: RunRadRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        log::debug!("Executing RAD request");
        Ok(self.run_rad_request(request))
    }
}
