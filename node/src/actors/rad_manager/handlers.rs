//! Message handlers for `RadManager`

use std::time::Duration;

use actix::{Handler, ResponseFuture};
use futures::FutureExt;

use witnet_data_structures::radon_report::{RadonReport, ReportContext};
use witnet_rad::{
    conditions::{evaluate_tally_precondition_clause, TallyPreconditionClauseResult},
    error::RadError,
    script::RadonScriptExecutionSettings,
    types::RadonTypes,
};
use witnet_validations::validations::run_tally;

use crate::actors::messages::{ResolveRA, RunTally};

use super::RadManager;

// This constant is used to ensure that a RetrievalTimeoutError is committed after 10 seconds
// This value must be lower than half an epoch, and having enough time to broadcasting the commit.
const MAX_RETRIEVAL_TIMEOUT: Duration = Duration::from_millis(10000);

impl Handler<ResolveRA> for RadManager {
    // This must be ResponseFuture, otherwise the actor dies on panic
    type Result = ResponseFuture<Result<RadonReport<RadonTypes>, RadError>>;

    fn handle(&mut self, msg: ResolveRA, _ctx: &mut Self::Context) -> Self::Result {
        // The result of the RAD aggregation is computed asynchronously, because the async block
        // returns a future
        let fut = async move {
            let sources = msg.rad_request.retrieve;
            let aggregator = msg.rad_request.aggregate;
            let active_wips = msg.active_wips.clone();
            // Add a timeout to each source retrieval
            // TODO: this timeout only works if there are no blocking operations.
            // Since currently the execution of RADON is blocking this thread, we can only
            // handle HTTP timeouts.
            // A simple fix would be to offload computation to another thread, to avoid blocking
            // the main thread. Then the timeout would apply to the message passing between threads.
            let timeout = match msg.timeout {
                None => MAX_RETRIEVAL_TIMEOUT,
                Some(timeout_from_config) => {
                    std::cmp::min(timeout_from_config, MAX_RETRIEVAL_TIMEOUT)
                }
            };

            let retrieve_responses_fut = sources
                .iter()
                .map(|retrieve| witnet_rad::run_retrieval(retrieve, active_wips.clone()))
                .map(|fut| {
                    tokio::time::timeout(timeout, fut).map(|response| {
                        // In case of timeout, set response to "RetrieveTimeout" error
                        response.unwrap_or(Err(RadError::RetrieveTimeout))
                    })
                });

            // Perform retrievals in parallel for the sake of synchronization between sources
            //  (increasing the likeliness of multiple sources returning results that are closer to each
            //  other).
            let retrieve_responses: Vec<RadonReport<RadonTypes>> =
                futures::future::join_all(retrieve_responses_fut)
                    .await
                    .into_iter()
                    .map(|result| RadonReport::from_result(result, &ReportContext::default()))
                    .collect();

            // Evaluate tally precondition to ensure that at least 20% of the data sources are not errors.
            // This stage does not need to evaluate the postcondition.
            let clause_result =
                evaluate_tally_precondition_clause(retrieve_responses, 0.2, 1, &msg.active_wips);
            match clause_result {
                Ok(TallyPreconditionClauseResult::MajorityOfValues {
                    values,
                    liars: _liars,
                    errors: _errors,
                }) => {
                    // Perform aggregation on the values that made it to the output vector after applying the
                    // source scripts (aka _normalization scripts_ in the original whitepaper) and filtering out
                    // failures.
                    let (res, _) = witnet_rad::run_aggregation_report(
                        values,
                        &aggregator,
                        RadonScriptExecutionSettings::all_but_partial_results(),
                        msg.active_wips,
                    );

                    res
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

        Box::pin(fut)
    }
}

impl Handler<RunTally> for RadManager {
    // This must be ResponseFuture, otherwise the actor dies on panic
    type Result = ResponseFuture<RadonReport<RadonTypes>>;

    fn handle(&mut self, msg: RunTally, _ctx: &mut Self::Context) -> Self::Result {
        let fut = async move {
            run_tally(
                msg.reports,
                &msg.script,
                msg.min_consensus_ratio,
                msg.commits_count,
                &msg.active_wips,
            )
        };

        Box::pin(fut)
    }
}

#[cfg(test)]
mod tests {
    use actix::{Actor, MailboxError, Message};

    use crate::utils::test_actix_system;

    use super::*;

    #[test]
    fn rad_manager_handler_can_panic() {
        // Ensure that the RadManager handlers can panic without shutting down the actor or the
        // entire actor system. In other words, the RadManager should be able to keep processing
        // messages after a panic.
        // This is only true for messages whose handler has type Result = ResponseFuture.
        // Other result types, including ResponseActFuture, will kill the actor on panic.

        // Create a dummy message that panics
        struct PanicMsg(String);

        impl Message for PanicMsg {
            type Result = String;
        }

        impl Handler<PanicMsg> for RadManager {
            // This must be ResponseFuture, otherwise the actor dies on panic
            type Result = ResponseFuture<String>;

            fn handle(&mut self, msg: PanicMsg, _ctx: &mut Self::Context) -> Self::Result {
                // This panic would kill the actor. Only panics inside a future are handled
                // correctly
                //panic!("{}", msg.0);
                let fut = async move {
                    panic!("{}", msg.0);
                };

                Box::pin(fut)
            }
        }

        // And another dummy message that does not panic
        struct DummyMsg(String);

        impl Message for DummyMsg {
            type Result = String;
        }

        impl Handler<DummyMsg> for RadManager {
            // This must be ResponseFuture, otherwise the actor dies on panic
            type Result = ResponseFuture<String>;

            fn handle(&mut self, msg: DummyMsg, _ctx: &mut Self::Context) -> Self::Result {
                let fut = async move { msg.0 };

                Box::pin(fut)
            }
        }

        test_actix_system(|| async move {
            let rad_manager = RadManager::default().start();
            let res = rad_manager
                .send(PanicMsg("message handler can panic".to_string()))
                .await;
            // The actor has panicked, so the result is Err(MailboxError)
            assert!(
                matches!(res, Err(MailboxError::Closed)),
                "expected `Err(MailboxError::Closed)`, got `{:?}`",
                res
            );

            // Try to send a new message to the actor
            let alive = "still alive".to_string();
            let res = rad_manager
                .send(DummyMsg(alive.clone()))
                .await
                .expect("mailbox error");
            // Results in success
            assert_eq!(res, alive);
        });
    }
}
