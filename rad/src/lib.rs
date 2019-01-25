//! # RAD Engine

use reqwest;

use witnet_data_structures::chain::RADRetrieve;
use witnet_data_structures::chain::RADType;
use witnet_data_structures::serializers::decoders::{TryFrom, TryInto};

use crate::error::{RadError, RadResult, WitnetError};
use crate::script::{execute_radon_script, unpack_radon_script};
use crate::types::{array::RadonArray, string::RadonString, RadonTypes};

pub mod error;
pub mod operators;
pub mod reducers;
pub mod script;
pub mod types;

/// Run retrieval stage of a data request.
pub fn run_retrieval(retrieve: RADRetrieve) -> RadResult<RadonTypes> {
    match retrieve.kind {
        RADType::HttpGet => {
            let response = reqwest::get(&retrieve.url)
                .map_err(|err| WitnetError::from(RadError::from(err)))?
                .text()
                .map_err(|err| WitnetError::from(RadError::from(err)))?;

            let input = RadonTypes::from(RadonString::from(response));
            let radon_script = unpack_radon_script(&retrieve.script)?;

            execute_radon_script(input, &radon_script)
        }
    }
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

#[test]
fn test_run_retrieval() {
    let script = vec![
        150, 83, 204, 132, 146, 1, 164, 109, 97, 105, 110, 204, 132, 146, 1, 164, 116, 101, 109,
        112, 204, 130,
    ];

    let retrieve = RADRetrieve {
        kind: RADType::HttpGet,
        url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
        script
    };

    let result = run_retrieval(retrieve).unwrap();

    match result {
        RadonTypes::Float(_) => {}
        err => panic!("Error in run_retrieval: {:?}", err),
    }
}
