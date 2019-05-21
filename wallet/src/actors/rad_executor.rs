//! # RAD requests executor
//!
//! Sync actor used to execute RAD-requests.
//! For more information see [`RadExecutor`](RadExecutor) struct.
use std::convert::TryFrom;
use std::thread;

use actix::prelude::*;
use rayon::prelude::*;

use witnet_data_structures::chain::RADRequest;
use witnet_rad::{self as rad, types::RadonTypes};

/// Actor that executes RAD-requests in a sync context.
pub struct RadExecutor {
    id: thread::ThreadId,
}

impl RadExecutor {
    /// Return an `Addr` to a thread-pool of `RadExecutor` actors with `num_threads` threads.
    pub fn start() -> Addr<Self> {
        SyncArbiter::start(num_cpus::get(), || Self {
            id: thread::current().id(),
        })
    }
}

impl Actor for RadExecutor {
    type Context = SyncContext<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("RAD-Executor started");
    }
}

/// Message to tell the RadExecutor to execute the containing RAD-request.
pub struct Run(pub RADRequest);

impl Message for Run {
    type Result = rad::Result<RadonTypes>;
}

impl Handler<Run> for RadExecutor {
    type Result = <Run as Message>::Result;

    fn handle(&mut self, Run(request): Run, _ctx: &mut Self::Context) -> Self::Result {
        log::debug!("[{:?}] Executing RAD request...", self.id);

        let res = request
            .retrieve
            .par_iter()
            .map(rad::run_retrieval)
            .collect::<Result<Vec<_>, _>>()
            .and_then(|retrievals| {
                rad::run_aggregation(retrievals, &request.aggregate)
                    .map_err(Into::into)
                    .and_then(|aggregated| {
                        RadonTypes::try_from(aggregated.as_slice())
                            .and_then(|aggregation_result| {
                                rad::run_consensus(vec![aggregation_result], &request.consensus)
                                    .and_then(|consensus_result| {
                                        RadonTypes::try_from(consensus_result.as_slice())
                                    })
                            })
                            .map_err(Into::into)
                    })
            });

        log::debug!("[{:?}] Finished RAD request.", self.id);

        res
    }
}
