//! Message handlers for `RadManager`

use actix::{Handler, Message};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use witnet_rad as rad;

use crate::actors::messages::{ResolveRA, RunTally};

use super::RadManager;

impl Handler<ResolveRA> for RadManager {
    type Result = <ResolveRA as Message>::Result;

    fn handle(&mut self, msg: ResolveRA, _ctx: &mut Self::Context) -> Self::Result {
        let sources = msg.rad_request.retrieve;
        let aggregator = msg.rad_request.aggregate;

        // Perform retrievals in parallel for the sake of synchronization between sources
        //  (increasing the likeliness of multiple sources returning results that are closer to each
        //  other).
        // FIXME: failed sources should not be ignored but rather passed along, as it is up to the
        //  aggregator script to decide how to handle source errors. Aggregators may ignore/drop
        //  errors by flattening the input array or just go ahead and try to apply a reducer that
        //  will surely fail in runtime because of lack of homogeneity in the array. In such case,
        //  we should make sure to throw the original source error, not the misleading "could not
        //  apply reducer on an array that is not homogeneous".
        let retrieve_responses = sources
            .par_iter()
            .filter_map(|source| {
                rad::run_retrieval(source)
                    .map_err(|err| {
                        log::error!("{:?}", err);
                    })
                    .ok()
            })
            .collect();

        // Perform aggregation on the values that made it to the output vector after applying the
        // source scripts (aka _normalization scripts_ in the original whitepaper) and filtering out
        // failures.
        rad::run_aggregation_report(retrieve_responses, &aggregator)
    }
}

impl Handler<RunTally> for RadManager {
    type Result = <RunTally as Message>::Result;

    fn handle(&mut self, msg: RunTally, _ctx: &mut Self::Context) -> Self::Result {
        let packed_script = msg.script;
        let reveals = msg.reveals;

        rad::run_tally_report(reveals, &packed_script)
    }
}
