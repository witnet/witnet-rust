//! # RAD requests executor
//!
//! Sync actor used to execute RAD-requests.
//! For more information see [`RadExecutor`](RadExecutor) struct.
use std::convert::TryFrom;

use actix::prelude::*;
use rayon::prelude::*;

use witnet_data_structures::chain::RADRequest;
use witnet_rad::{self as rad, types::RadonTypes};

mod handlers;

pub use handlers::*;

/// Actor that executes RAD-requests in a sync context.
pub struct RadExecutor;

impl RadExecutor {
    /// Start actor.
    pub fn start() -> Addr<Self> {
        SyncArbiter::start(1, || Self)
    }

    /// Run RAD request
    pub fn run(&self, request: RADRequest) -> rad::Result<RadonTypes> {
        request
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
            })
    }
}

impl Actor for RadExecutor {
    type Context = SyncContext<Self>;
}
