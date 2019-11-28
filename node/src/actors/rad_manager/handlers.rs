//! Message handlers for `RadManager`

use actix::{Handler, Message, ResponseFuture};

use crate::actors::messages::{ResolveRA, RunTally};
use futures::Future;
use tokio::util::FutureExt;
use witnet_data_structures::radon_report::RadonReport;
use witnet_rad::error::RadError;

use super::RadManager;
use witnet_rad::types::RadonTypes;

impl Handler<ResolveRA> for RadManager {
    type Result = ResponseFuture<RadonReport<RadonTypes>, RadError>;

    fn handle(&mut self, msg: ResolveRA, _ctx: &mut Self::Context) -> Self::Result {
        let timeout = msg.timeout;
        // The result of the RAD aggregation is computed asynchronously, because the async block
        // returns a std future. It is called fut03 because it uses the 0.3 version of futures,
        // while most of our codebase is still on 0.1 futures.
        let fut03 = async {
            let dr_pointer = msg.dr_pointer;
            let sources = msg.rad_request.retrieve;
            let aggregator = msg.rad_request.aggregate;

            let retrieve_responses_fut = sources
                .iter()
                .map(|retrieve| witnet_rad::run_retrieval(retrieve));

            // Perform retrievals in parallel for the sake of synchronization between sources
            //  (increasing the likeliness of multiple sources returning results that are closer to each
            //  other).
            let retrieve_responses = futures03::future::join_all(retrieve_responses_fut)
                .await
                .into_iter()
                // FIXME: failed sources should not be ignored but rather passed along, as it is up to the
                //  aggregator script to decide how to handle source errors. Aggregators may ignore/drop
                //  errors by flattening the input array or just go ahead and try to apply a reducer that
                //  will surely fail in runtime because of lack of homogeneity in the array. In such case,
                //  we should make sure to throw the original source error, not the misleading "could not
                //  apply reducer on an array that is not homogeneous".
                .filter_map(|retrieve| {
                    retrieve
                        .map_err(|err| {
                            log::warn!(
                                "Failed to run retrieval for data request {}: {}",
                                dr_pointer,
                                err
                            );
                        })
                        .ok()
                })
                .collect();

            // Perform aggregation on the values that made it to the output vector after applying the
            // source scripts (aka _normalization scripts_ in the original whitepaper) and filtering out
            // failures.
            witnet_rad::run_aggregation_report(retrieve_responses, &aggregator)
        };

        // Magic conversion from std::future::Future (futures 0.3) and futures::Future (futures 0.1)
        let fut = futures_util::compat::Compat::new(Box::pin(fut03));

        if let Some(timeout) = timeout {
            // Add timeout, if there is one
            // TODO: this timeout only works if there are no blocking operations.
            // Since currently the execution of RADON is blocking this thread, we can only
            // handle HTTP timeouts.
            // A simple fix would be to offload computation to another thread, to avoid blocking
            // the main thread. Then the timeout would apply to the message passing between threads.
            Box::new(fut.timeout(timeout).map_err(|error| {
                if error.is_elapsed() {
                    RadError::RetrieveTimeout
                } else if error.is_inner() {
                    error.into_inner().unwrap()
                } else {
                    panic!("Unhandled tokio timer error");
                }
            }))
        } else {
            Box::new(fut)
        }
    }
}

impl Handler<RunTally> for RadManager {
    type Result = <RunTally as Message>::Result;

    fn handle(&mut self, msg: RunTally, _ctx: &mut Self::Context) -> Self::Result {
        let packed_script = msg.script;
        let reveals = msg.reveals;

        witnet_rad::run_tally_report(reveals, &packed_script)
    }
}
