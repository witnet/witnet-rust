//! Message handlers for `RadManager`

use actix::{Handler, Message, ResponseFuture};

use super::RadManager;

use crate::actors::messages::{ResolveRA, RunTally};
use futures::Future;
use tokio::util::FutureExt;
use witnet_data_structures::radon_report::{RadonReport, ReportContext, Stage, TallyMetaData};
use witnet_rad::{error::RadError, types::RadonTypes};
use witnet_validations::validations::tally_precondition_clause;

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
            let retrieve_responses: Vec<RadonReport<RadonTypes>> =
                futures03::future::join_all(retrieve_responses_fut)
                    .await
                    .into_iter()
                    .filter_map(|retrieve| {
                        let rad_report =
                            RadonReport::from_result(retrieve, &ReportContext::default());
                        match rad_report {
                            Ok(x) => Some(x),
                            Err(err) => {
                                log::warn!(
                                    "Failed to run retrieval for data request {}: {}",
                                    dr_pointer,
                                    err
                                );
                                None
                            }
                        }
                    })
                    .collect();

            let clause_result = tally_precondition_clause(retrieve_responses, 0.2);

            match clause_result {
                Ok((radon_types_vec, _)) => {
                    // Perform aggregation on the values that made it to the output vector after applying the
                    // source scripts (aka _normalization scripts_ in the original whitepaper) and filtering out
                    // failures.
                    witnet_rad::run_aggregation_report(radon_types_vec, &aggregator)
                }
                Err(e) => RadonReport::from_result(Err(e), &ReportContext::default()),
            }
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
        let liars = msg.liars;

        match reveals {
            Ok(reveals) => witnet_rad::run_tally_report(reveals, &packed_script, Some(liars)),
            Err(e) => {
                let context = &mut ReportContext::default();
                let mut metadata = TallyMetaData::default();
                metadata.liars = liars;
                context.stage = Stage::Tally(metadata);

                RadonReport::from_result(Err(e), &context)
            }
        }
    }
}
