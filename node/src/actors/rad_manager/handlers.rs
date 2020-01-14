//! Message handlers for `RadManager`

use actix::{Handler, Message, ResponseFuture};

use super::RadManager;

use crate::actors::messages::{ResolveRA, RunTally};
use futures::Future;
use tokio::util::FutureExt;
use witnet_data_structures::radon_report::{RadonReport, ReportContext, Stage, TallyMetaData};
use witnet_rad::{error::RadError, run_tally_report, types::RadonTypes};
use witnet_validations::validations::{
    evaluate_tally_precondition_clause, TallyPreconditionClauseResult,
};

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

            let clause_result = evaluate_tally_precondition_clause(retrieve_responses, 0.2);

            match clause_result {
                Ok(TallyPreconditionClauseResult::MajorityOfValues {
                    values,
                    liars: _liars,
                }) => {
                    // Perform aggregation on the values that made it to the output vector after applying the
                    // source scripts (aka _normalization scripts_ in the original whitepaper) and filtering out
                    // failures.
                    witnet_rad::run_aggregation_report(values, &aggregator)
                }
                Ok(TallyPreconditionClauseResult::MajorityOfErrors { errors_mode }) => {
                    RadonReport::from_result(
                        Ok(RadonTypes::RadonError(errors_mode)),
                        &ReportContext::default(),
                    )
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

    // TODO: replace the body of this handler with a simple call to `validations::validate_consensus`
    fn handle(&mut self, msg: RunTally, _ctx: &mut Self::Context) -> Self::Result {
        let packed_script = msg.script;
        let reports = msg.reports;

        let clause_result =
            evaluate_tally_precondition_clause(reports.clone(), msg.min_consensus_ratio);

        match clause_result {
            // The reveals passed the precondition clause (a parametric majority of them were successful
            // values). Run the tally, which will add more liars if any.
            Ok(TallyPreconditionClauseResult::MajorityOfValues { values, liars }) => {
                run_tally_report(values, &packed_script, Some(liars))
            }
            // The reveals did not pass the precondition clause (a parametric majority of them were
            // errors). Tally will not be run, and the mode of the errors will be committed.
            Ok(TallyPreconditionClauseResult::MajorityOfErrors { errors_mode }) => {
                // Do not impose penalties on any of the revealers.
                let mut metadata = TallyMetaData::default();
                metadata.update_liars(vec![false; reports.len()]);

                RadonReport::from_result(
                    Ok(RadonTypes::RadonError(errors_mode)),
                    &ReportContext::from_stage(Stage::Tally(metadata)),
                )
            }
            // Failed to evaluate the precondition clause. `RadonReport::from_result()?` is the last
            // chance for errors to be intercepted and used for consensus.
            Err(e) => {
                let mut metadata = TallyMetaData::default();
                metadata.liars = vec![false; reports.len()];

                RadonReport::from_result(Err(e), &ReportContext::from_stage(Stage::Tally(metadata)))
            }
        }
    }
}
