//! Message handlers for `RadManager`
use super::{messages, RadManager};
use actix::{Handler, Message};
use witnet_rad as rad;

impl Handler<messages::ResolveRA> for RadManager {
    type Result = <messages::ResolveRA as Message>::Result;

    fn handle(&mut self, msg: messages::ResolveRA, _ctx: &mut Self::Context) -> Self::Result {
        let retrieve_scripts = msg.rad_request.retrieve;
        let aggregate_script = msg.rad_request.aggregate.script;

        let retrieve_responses = retrieve_scripts
            .into_iter()
            .filter_map(|retrieve| rad::run_retrieval(retrieve).ok())
            .collect();

        rad::run_aggregation(retrieve_responses, aggregate_script)
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
