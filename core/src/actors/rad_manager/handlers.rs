//! Message handlers for `RadManager`
use super::{messages, RadManager};
use actix::{Handler, Message};
use log;
use witnet_rad as rad;

impl Handler<messages::ResolveRA> for RadManager {
    type Result = <messages::ResolveRA as Message>::Result;

    fn handle(&mut self, _msg: messages::ResolveRA, _ctx: &mut Self::Context) -> Self::Result {
        log::warn!("ResolveRA: unimplemented handler!");
        rad::run_retrieval();
        rad::run_aggregation();

        Ok(Vec::new())
    }
}

impl Handler<messages::RunConsensus> for RadManager {
    type Result = <messages::RunConsensus as Message>::Result;

    fn handle(&mut self, msg: messages::RunConsensus, _ctx: &mut Self::Context) -> Self::Result {
        let packed_script = msg.script.script;
        let reveals = msg.reveals;

        rad::run_consensus(reveals, packed_script)
    }
}
