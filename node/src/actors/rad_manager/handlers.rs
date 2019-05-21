//! Message handlers for `RadManager`
use actix::{Handler, Message};
use std::convert::TryFrom;
use witnet_rad as rad;
use witnet_rad::types::RadonTypes;

use super::RadManager;
use crate::actors::messages::{ResolveRA, RunConsensus};

impl Handler<ResolveRA> for RadManager {
    type Result = <ResolveRA as Message>::Result;

    fn handle(&mut self, msg: ResolveRA, _ctx: &mut Self::Context) -> Self::Result {
        let retrieve_scripts = msg.rad_request.retrieve;
        let aggregate_script = msg.rad_request.aggregate;

        let retrieve_responses = retrieve_scripts
            .iter()
            .filter_map(|retrieve| rad::run_retrieval(retrieve).ok())
            .collect();

        rad::run_aggregation(retrieve_responses, &aggregate_script)
    }
}

impl Handler<RunConsensus> for RadManager {
    type Result = <RunConsensus as Message>::Result;

    fn handle(&mut self, msg: RunConsensus, _ctx: &mut Self::Context) -> Self::Result {
        let packed_script = msg.script;
        let reveals = msg.reveals;

        let radon_types_vec: Vec<RadonTypes> = reveals
            .iter()
            .filter_map(|input| RadonTypes::try_from(input.as_slice()).ok())
            .collect();

        rad::run_consensus(radon_types_vec, &packed_script)
    }
}
