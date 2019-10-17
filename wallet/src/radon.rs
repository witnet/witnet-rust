use std::convert::TryFrom;

use rayon::prelude::*;

use crate::*;

// TODO: Use a timeout when running the rad request
pub fn run_request(request: &types::RADRequest) -> Result<types::RadonTypes, types::RadError> {
    request
        .retrieve
        .par_iter()
        .map(witnet_rad::run_retrieval)
        .collect::<Result<Vec<_>, _>>()
        .and_then(|retrievals| {
            witnet_rad::run_aggregation(retrievals, &request.aggregate)
                .map_err(Into::into)
                .and_then(|aggregated| {
                    types::RadonTypes::try_from(aggregated.as_slice())
                        .and_then(|aggregation_result| {
                            witnet_rad::run_consensus(vec![aggregation_result], &request.tally)
                                .and_then(|consensus_result| {
                                    types::RadonTypes::try_from(consensus_result.as_slice())
                                })
                        })
                        .map_err(Into::into)
                })
        })
}
