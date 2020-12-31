//! Message handlers for `RadManager`

use actix::{Handler, Message, ResponseFuture};
use witnet_data_structures::radon_report::{RadonReport, ReportContext};
use witnet_rad::{error::RadError, script::RadonScriptExecutionSettings, types::RadonTypes};
use witnet_validations::validations::{
    construct_report_from_clause_result, evaluate_tally_precondition_clause,
    TallyPreconditionClauseResult,
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

            let clause_result = evaluate_tally_precondition_clause(retrieve_responses, 0.2, 1);

            match clause_result {
                Ok(TallyPreconditionClauseResult::MajorityOfValues {
                    values,
                    liars: _liars,
                    errors: _errors,
                }) => {
                    // Perform aggregation on the values that made it to the output vector after applying the
                    // source scripts (aka _normalization scripts_ in the original whitepaper) and filtering out
                    // failures.
                    witnet_rad::run_aggregation_report(
                        values,
                        &aggregator,
                        RadonScriptExecutionSettings::all_but_partial_results(),
                    )
                }
                Ok(TallyPreconditionClauseResult::MajorityOfErrors { errors_mode }) => {
                    Ok(RadonReport::from_result(
                        Ok(RadonTypes::RadonError(errors_mode)),
                        &ReportContext::default(),
                    ))
                }
                Err(e) => Ok(RadonReport::from_result(Err(e), &ReportContext::default())),
            }
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
        let packed_script = msg.script;
        let reports = msg.reports;

        let reports_len = reports.len();
        let clause_result =
            evaluate_tally_precondition_clause(reports, msg.min_consensus_ratio, msg.commits_count);

        construct_report_from_clause_result(clause_result, &packed_script, reports_len)
    }
}
