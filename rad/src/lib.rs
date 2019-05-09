//! # RAD Engine

use reqwest;
use std::convert::TryInto;

use witnet_data_structures::chain::{RADRetrieve, RADType};

use crate::error::RadError;
use crate::script::{execute_radon_script, unpack_radon_script};
use crate::types::{array::RadonArray, string::RadonString, RadonTypes};

pub mod error;
pub mod hash_functions;
pub mod operators;
pub mod reducers;
pub mod script;
pub mod types;

/// Run retrieval stage of a data request.
pub fn run_retrieval(retrieve: RADRetrieve) -> Result<RadonTypes, RadError> {
    match retrieve.kind {
        RADType::HttpGet => {
            let response = reqwest::get(&retrieve.url)
                .map_err(RadError::from)?
                .text()
                .map_err(RadError::from)?;

            let input = RadonTypes::from(RadonString::from(response));
            let radon_script = unpack_radon_script(&retrieve.script)?;

            execute_radon_script(input, &radon_script)
        }
    }
}

/// Run aggregate stage of a data request.
pub fn run_aggregation(
    radon_types_vec: Vec<RadonTypes>,
    script: Vec<u8>,
) -> Result<Vec<u8>, RadError> {
    let radon_script = unpack_radon_script(&script)?;

    let radon_array = RadonArray::from(radon_types_vec);

    let rad_aggregation: RadonTypes =
        execute_radon_script(RadonTypes::from(radon_array), &radon_script)?;

    rad_aggregation.try_into().map_err(Into::into)
}

/// Run consensus stage of a data request.
pub fn run_consensus(
    radon_types_vec: Vec<RadonTypes>,
    script: Vec<u8>,
) -> Result<Vec<u8>, RadError> {
    let radon_script = unpack_radon_script(&script)?;

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

#[test]
fn test_run_consensus_and_aggregation() {
    use crate::types::float::RadonFloat;

    let f_1 = RadonTypes::Float(RadonFloat::from(1f64));
    let f_3 = RadonTypes::Float(RadonFloat::from(3f64));

    let radon_types_vec = vec![f_1, f_3];

    let packed_script = [145, 146, 102, 32].to_vec();

    let expected = RadonTypes::Float(RadonFloat::from(2f64)).try_into().ok();

    let output_consensus = run_consensus(radon_types_vec.clone(), packed_script.clone()).ok();
    let output_aggregate = run_aggregation(radon_types_vec, packed_script).ok();

    assert_eq!(output_consensus, expected);
    assert_eq!(output_aggregate, expected);
}

#[test]
#[ignore]
fn test_run_retrieval_random_api() {
    let script = vec![
        149, 83, 204, 132, 146, 1, 164, 100, 97, 116, 97, 204, 128, 146, 1, 0,
    ];
    let retrieve = RADRetrieve {
        kind: RADType::HttpGet,
        url: "http://qrng.anu.edu.au/API/jsonI.php?length=1&type=uint8".to_string(),
        script,
    };

    let result = run_retrieval(retrieve).unwrap();

    match result {
        RadonTypes::Float(_) => {}
        err => panic!("Error in run_retrieval: {:?}", err),
    }
}
