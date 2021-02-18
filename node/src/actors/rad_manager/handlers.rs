//! Message handlers for `RadManager`

use actix::{Handler, Message, ResponseFuture};
use witnet_data_structures::radon_report::{RadonReport, ReportContext};
use witnet_rad::{error::RadError, types::RadonTypes};
use witnet_validations::validations::{
    run_aggregation_with_precondition, run_tally_with_precondition,
};

use crate::actors::messages::{ResolveRA, RunTally};

use super::RadManager;
use futures::FutureExt;

impl Handler<ResolveRA> for RadManager {
    type Result = ResponseFuture<Result<RadonReport<RadonTypes>, RadError>>;

    fn handle(&mut self, msg: ResolveRA, _ctx: &mut Self::Context) -> Self::Result {
        let timeout = msg.timeout;
        // The result of the RAD aggregation is computed asynchronously, because the async block
        // returns a future
        let fut = async {
            let sources = msg.rad_request.retrieve;
            let aggregator = msg.rad_request.aggregate;

            let retrieve_responses_fut = sources
                .iter()
                .map(|retrieve| witnet_rad::run_retrieval(retrieve));

            // Perform retrievals in parallel for the sake of synchronization between sources
            //  (increasing the likeliness of multiple sources returning results that are closer to each
            //  other).
            let retrieve_responses: Vec<RadonReport<RadonTypes>> =
                futures::future::join_all(retrieve_responses_fut)
                    .await
                    .into_iter()
                    .map(|retrieve| RadonReport::from_result(retrieve, &ReportContext::default()))
                    .collect();

            run_aggregation_with_precondition(&aggregator, retrieve_responses)
        };

        if let Some(timeout) = timeout {
            // Add timeout, if there is one
            // TODO: this timeout only works if there are no blocking operations.
            // Since currently the execution of RADON is blocking this thread, we can only
            // handle HTTP timeouts.
            // A simple fix would be to offload computation to another thread, to avoid blocking
            // the main thread. Then the timeout would apply to the message passing between threads.
            Box::pin(
                tokio::time::timeout(timeout, fut).map(|result| match result {
                    Ok(Ok(x)) => Ok(x),
                    Ok(Err(rad_error)) => Err(rad_error),
                    Err(_elapsed) => Ok(RadonReport::from_result(
                        Err(RadError::RetrieveTimeout),
                        &ReportContext::default(),
                    )),
                }),
            )
        } else {
            Box::pin(fut)
        }
    }
}

impl Handler<RunTally> for RadManager {
    type Result = <RunTally as Message>::Result;

    fn handle(&mut self, msg: RunTally, _ctx: &mut Self::Context) -> Self::Result {
        run_tally_with_precondition(
            &msg.script,
            msg.reports,
            msg.min_consensus_ratio,
            msg.commits_count,
        )
    }
}
