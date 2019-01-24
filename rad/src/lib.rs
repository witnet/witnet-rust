//! # RAD Engine

use crate::error::RadResult;
use crate::script::{execute_radon_script, unpack_radon_script};
use crate::types::{array::RadonArray, string::RadonString, RadonTypes};
use witnet_data_structures::chain::RADRetrieve;
use witnet_data_structures::serializers::decoders::{TryFrom, TryInto};

pub mod error;
pub mod operators;
pub mod reducers;
pub mod script;
pub mod types;

/// Run retrieval stage of a data request.
pub fn run_retrieval(_retrieve: RADRetrieve) -> RadResult<RadonTypes> {
    // TODO: HTTP Getter
    Ok(RadonTypes::String(RadonString::from(String::from("Sunny"))))
}

/// Run aggregate stage of a data request.
pub fn run_aggregation(radon_types_vec: Vec<RadonTypes>, script: Vec<u8>) -> RadResult<Vec<u8>> {
    let radon_script = unpack_radon_script(&script)?;

    let radon_array = RadonArray::from(radon_types_vec);

    let rad_consensus: RadonTypes =
        execute_radon_script(RadonTypes::from(radon_array), &radon_script)?;

    rad_consensus.try_into().map_err(Into::into)
}

/// Run consensus stage of a data request.
pub fn run_consensus(inputs: Vec<Vec<u8>>, script: Vec<u8>) -> RadResult<Vec<u8>> {
    let radon_script = unpack_radon_script(&script)?;

    let radon_types_vec: Vec<RadonTypes> = inputs
        .iter()
        .filter_map(|input| RadonTypes::try_from(input.as_slice()).ok())
        .collect();

    let radon_array = RadonArray::from(radon_types_vec);

    let rad_consensus: RadonTypes =
        execute_radon_script(RadonTypes::from(radon_array), &radon_script)?;

    rad_consensus.try_into().map_err(Into::into)
}

/// Run deliver clauses of a data request.
pub fn run_delivery() {}
