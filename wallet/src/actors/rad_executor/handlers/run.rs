use actix::prelude::*;

use witnet_data_structures::chain::RADRequest;
use witnet_rad::{self as rad, types::RadonTypes};

use crate::actors::RadExecutor;

/// Execute the containing RAD-request.
pub struct Run(pub RADRequest);

impl Message for Run {
    type Result = rad::Result<RadonTypes>;
}

impl Handler<Run> for RadExecutor {
    type Result = <Run as Message>::Result;

    fn handle(&mut self, Run(request): Run, _ctx: &mut Self::Context) -> Self::Result {
        log::debug!("Executing RAD request");
        self.run(request)
    }
}
