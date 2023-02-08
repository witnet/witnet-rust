//! Message handlers for `RadManager`

use std::time::Duration;

use actix::{Handler, ResponseFuture};
use futures::FutureExt;
use witnet_data_structures::radon_report::{RadonReport, ReportContext, RetrievalMetadata, Stage};
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
        // Fetching these values this early makes lifetimes easier for the fut block below
        let witnessing = self.witnessing.clone();

        // The result of the RAD aggregation is computed asynchronously, because the async block
        // returns a future
        let fut = async move {
            let sources = msg.rad_request.retrieve;
            let aggregate = msg.rad_request.aggregate;
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
            let settings = RadonScriptExecutionSettings::disable_all();
            let retrieval_context =
                ReportContext::from_stage(Stage::Retrieval(RetrievalMetadata::default()));
            let retrieve_responses_fut = sources
                .iter()
                .map(|retrieve| {
                    witnet_rad::run_paranoid_retrieval(
                        retrieve,
                        aggregate.clone(),
                        settings,
                        active_wips.clone(),
                        witnessing.clone(),
                    )
                })
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
                    .map(|retrieve| {
                        retrieve.unwrap_or_else(|error| {
                            RadonReport::from_result(Err(error), &retrieval_context)
                        })
                    })
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
                    let (aggregation_result, aggregation_context) =
                        witnet_rad::run_aggregation_report(
                            values,
                            aggregate,
                            RadonScriptExecutionSettings::all_but_partial_results(),
                            &msg.active_wips,
                        );

                    // Convert Err into Ok because returning Err from this handler means that the
                    // node should not commit the result, and we do want to commit this error.
                    Ok(aggregation_result.unwrap_or_else(|error| {
                        RadonReport::from_result(Err(error), &aggregation_context)
                    }))
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
    use witnet_data_structures::chain::{
        tapi::all_wips_active, RADAggregate, RADRequest, RADRetrieve, RADTally, RADType,
    };
    use witnet_rad::reducers::RadonReducers;

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

    #[test]
    fn retrieval_http_error() {
        // HTTP errors should not short-circuit data request execution
        test_actix_system(|| async move {
            let rad_manager = RadManager::default().start();
            let rad_request = RADRequest {
                time_lock: 0,
                // One HTTP error source and 2 RNG sources
                retrieve: vec![
                    RADRetrieve {
                        kind: RADType::HttpGet,
                        // Invalid URL to trigger HTTP error
                        url: "a".to_string(),
                        script: vec![128],
                        body: vec![],
                        headers: vec![],
                    },
                    RADRetrieve {
                        kind: RADType::Rng,
                        url: "".to_string(),
                        script: vec![128],
                        body: vec![],
                        headers: vec![],
                    },
                    RADRetrieve {
                        kind: RADType::Rng,
                        url: "".to_string(),
                        script: vec![128],
                        body: vec![],
                        headers: vec![],
                    },
                ],
                aggregate: RADAggregate {
                    filters: vec![],
                    reducer: RadonReducers::HashConcatenate as u32,
                },
                tally: RADTally {
                    filters: vec![],
                    reducer: RadonReducers::Mode as u32,
                },
            };
            let active_wips = all_wips_active();
            let res = rad_manager
                .send(ResolveRA {
                    rad_request,
                    timeout: None,
                    active_wips,
                })
                .await
                .unwrap()
                .unwrap();

            assert!(matches!(res.into_inner(), RadonTypes::Bytes(..)));
        });
    }

    #[test]
    fn aggregation_error() {
        test_actix_system(|| async move {
            let rad_manager = RadManager::default().start();
            let rad_request = RADRequest {
                time_lock: 0,
                retrieve: vec![RADRetrieve {
                    kind: RADType::Rng,
                    url: "".to_string(),
                    script: vec![128],
                    body: vec![],
                    headers: vec![],
                }],
                aggregate: RADAggregate {
                    filters: vec![],
                    // Use invalid reducer to simulate error in aggregation function, although such
                    // a request is invalid and cannot be included in a block
                    reducer: u32::MAX,
                },
                tally: RADTally {
                    filters: vec![],
                    reducer: RadonReducers::HashConcatenate as u32,
                },
            };
            let active_wips = all_wips_active();
            let res = rad_manager
                .send(ResolveRA {
                    rad_request,
                    timeout: None,
                    active_wips,
                })
                .await
                .unwrap()
                .unwrap();

            assert!(matches!(res.into_inner(), RadonTypes::RadonError(..)));
        });
    }
}
