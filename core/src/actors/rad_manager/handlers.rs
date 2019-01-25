//! Message handlers for `RadManager`
use super::{messages, RadManager};
use actix::{Handler, Message};
use witnet_data_structures::serializers::decoders::TryFrom;
use witnet_rad as rad;
use witnet_rad::types::RadonTypes;

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

        let radon_types_vec: Vec<RadonTypes> = reveals
            .iter()
            .filter_map(|input| RadonTypes::try_from(input.as_slice()).ok())
            .collect();

        rad::run_consensus(radon_types_vec, packed_script)
    }
}
