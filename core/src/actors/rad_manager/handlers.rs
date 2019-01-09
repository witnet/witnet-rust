//! Message handlers for `RadManager`
use super::{messages, RadManager};
use actix::Handler;
use log;
use witnet_radon as radon;

impl Handler<messages::ResolveRA> for RadManager {
    type Result = ();

    fn handle(&mut self, _msg: messages::ResolveRA, _ctx: &mut Self::Context) {
        log::warn!("ResolveRA: unimplemented handler!");
        radon::run_retrieval();
        radon::run_aggregate();
    }
}

impl Handler<messages::RunConsensus> for RadManager {
    type Result = ();

    fn handle(&mut self, _msg: messages::RunConsensus, _ctx: &mut Self::Context) {
        log::warn!("RunConsensus: unimplemented handler!");
        radon::run_consensus();
    }
}
