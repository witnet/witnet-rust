//! Message handlers for `RadManager`

use actix::{Handler, Message, ResponseFuture};

use witnet_data_structures::radon_report::RadonReport;
use witnet_rad::{error::RadError, types::RadonTypes};

use crate::actors::messages::{ResolveRA, RunTally};

use super::RadManager;

impl Handler<ResolveRA> for RadManager {
    type Result = ResponseFuture<RadonReport<RadonTypes>, RadError>;

    fn handle(&mut self, msg: ResolveRA, _ctx: &mut Self::Context) -> Self::Result {
        self.resolve_ra(msg)
    }
}

impl Handler<RunTally> for RadManager {
    type Result = <RunTally as Message>::Result;

    fn handle(&mut self, msg: RunTally, _ctx: &mut Self::Context) -> Self::Result {
        self.run_tally(msg)
    }
}
