//! # RAD Engine

use std::convert::TryInto;

use reqwest;

use crate::error::RadError;
use crate::script::{execute_radon_script, unpack_radon_script};
use crate::types::{array::RadonArray, string::RadonString, RadonTypes};
use witnet_data_structures::chain::{RADAggregate, RADConsensus, RADRetrieve, RADType};

pub mod error;
pub mod hash_functions;
pub mod operators;
pub mod reducers;
pub mod script;
pub mod types;

pub type Result<T> = std::result::Result<T, RadError>;

/// Run retrieval stage of a data request.
pub fn run_retrieval(retrieve: &RADRetrieve) -> Result<RadonTypes> {
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
    aggregate: &RADAggregate,
) -> Result<Vec<u8>> {
    log::debug!("run_aggregation: {:?}", radon_types_vec);
    let radon_script = unpack_radon_script(aggregate.script.as_slice())?;

    let radon_array = RadonArray::from(radon_types_vec);

    let rad_aggregation: RadonTypes =
        execute_radon_script(RadonTypes::from(radon_array), &radon_script)?;

    rad_aggregation.try_into().map_err(Into::into)
}

/// Run consensus stage of a data request.
pub fn run_consensus(
    radon_types_vec: Vec<RadonTypes>,
    consensus: &RADConsensus,
) -> Result<Vec<u8>> {
    let radon_script = unpack_radon_script(consensus.script.as_slice())?;

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
        134, 24, 67, 24, 116, 130, 1, 100, 109, 97, 105, 110, 24, 116, 130, 1, 100, 116, 101, 109,
        112, 24, 114,
    ];

    let retrieve = RADRetrieve {
        kind: RADType::HttpGet,
        url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
        script
    };

    let result = run_retrieval(&retrieve).unwrap();

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

    let packed_script = vec![129, 130, 24, 86, 3];

    let expected = RadonTypes::Float(RadonFloat::from(2f64)).try_into();

    let output_aggregate = run_aggregation(
        radon_types_vec.clone(),
        &RADAggregate {
            script: packed_script.clone(),
        },
    );
    let output_consensus = run_consensus(
        radon_types_vec,
        &RADConsensus {
            script: packed_script,
        },
    );

    assert_eq!(output_aggregate, expected);
    assert_eq!(output_consensus, expected);
}

#[test]
#[ignore]
fn test_run_retrieval_random_api() {
    let script = vec![
        133, 24, 83, 24, 132, 130, 1, 100, 100, 97, 116, 97, 24, 128, 130, 1, 0,
    ];
    let retrieve = RADRetrieve {
        kind: RADType::HttpGet,
        url: "http://qrng.anu.edu.au/API/jsonI.php?length=1&type=uint8".to_string(),
        script,
    };

    let result = run_retrieval(&retrieve).unwrap();

    match result {
        RadonTypes::Float(_) => {}
        err => panic!("Error in run_retrieval: {:?}", err),
    }
}

#[test]
fn test_run_all_risk_premium() {
    use std::convert::TryFrom;

    let retrieve = RADRetrieve {
        kind: RADType::HttpGet,
        url: "https://wrapapi.com/use/aesedepece/ffzz/prima/0.0.3?wrapAPIKey=ql4DVWylABdXCpt1NUTLNEDwPH57aHGm".to_string(),
        script: vec![129, 24, 70],
    };
    let aggregate = RADAggregate {
        script: vec![129, 130, 24, 86, 3],
    };
    let tally = RADConsensus {
        script: vec![130, 130, 24, 86, 3, 130, 24, 52, 24, 80],
    };

    let retrieved = run_retrieval(&retrieve).unwrap();
    let aggregated = RadonTypes::try_from(
        run_aggregation(vec![retrieved], &aggregate)
            .unwrap()
            .as_slice(),
    )
    .unwrap();
    let tallied =
        RadonTypes::try_from(run_consensus(vec![aggregated], &tally).unwrap().as_slice()).unwrap();

    match tallied {
        RadonTypes::Boolean(_) => {}
        err => panic!("Error in run_retrieval: {:?}", err),
    }
}

#[test]
fn test_run_all_murders() {
    use std::convert::TryFrom;

    let retrieve = RADRetrieve {
        kind: RADType::HttpGet,
        url: "https://wrapapi.com/use/aesedepece/ffzz/murders/0.0.2?wrapAPIKey=ql4DVWylABdXCpt1NUTLNEDwPH57aHGm".to_string(),
        script: vec![129, 24, 70],
    };
    let aggregate = RADAggregate {
        script: vec![129, 130, 24, 86, 3],
    };
    let tally = RADConsensus {
        script: vec![130, 130, 24, 86, 3, 130, 24, 52, 24, 200],
    };

    let retrieved = run_retrieval(&retrieve).unwrap();
    let aggregated = RadonTypes::try_from(
        run_aggregation(vec![retrieved], &aggregate)
            .unwrap()
            .as_slice(),
    )
    .unwrap();
    let tallied =
        RadonTypes::try_from(run_consensus(vec![aggregated], &tally).unwrap().as_slice()).unwrap();

    match tallied {
        RadonTypes::Boolean(_) => {}
        err => panic!("Error in run_retrieval: {:?}", err),
    }
}

#[test]
fn test_run_all_air_quality() {
    use std::convert::TryFrom;

    let retrieve = RADRetrieve {
        kind: RADType::HttpGet,
        url: "http://airemadrid.herokuapp.com/api/estacion".to_string(),
        script: vec![
            135, 24, 67, 24, 112, 130, 24, 84, 0, 130, 24, 97, 101, 104, 111, 114, 97, 48, 24, 116,
            130, 24, 97, 101, 118, 97, 108, 111, 114, 24, 114,
        ],
    };
    let aggregate = RADAggregate {
        script: vec![129, 130, 24, 86, 3],
    };
    let tally = RADConsensus {
        script: vec![130, 130, 24, 86, 3, 130, 24, 52, 10],
    };

    let retrieved = run_retrieval(&retrieve).unwrap();
    let aggregated = RadonTypes::try_from(
        run_aggregation(vec![retrieved], &aggregate)
            .unwrap()
            .as_slice(),
    )
    .unwrap();
    let tallied =
        RadonTypes::try_from(run_consensus(vec![aggregated], &tally).unwrap().as_slice()).unwrap();

    match tallied {
        RadonTypes::Boolean(_) => {}
        err => panic!("Error in run_retrieval: {:?}", err),
    }
}

#[test]
fn test_run_all_elections() {
    use crate::types::RadonType;
    use std::convert::TryFrom;

    let retrieve = RADRetrieve {
        kind: RADType::HttpGet,
        url: "https://wrapapi.com/use/aesedepece/ffzz/generales/0.0.3?wrapAPIKey=ql4DVWylABdXCpt1NUTLNEDwPH57aHGm".to_string(),
        script: vec![132, 24, 67, 24, 116, 130, 24, 97, 100, 80, 83, 79, 69, 24, 114],
    };
    let aggregate = RADAggregate {
        script: vec![129, 130, 24, 86, 3],
    };
    let tally = RADConsensus {
        script: vec![129, 130, 24, 86, 3],
    };

    let retrieved = run_retrieval(&retrieve).unwrap();
    let aggregated = RadonTypes::try_from(
        run_aggregation(vec![retrieved], &aggregate)
            .unwrap()
            .as_slice(),
    )
    .unwrap();
    let tallied =
        RadonTypes::try_from(run_consensus(vec![aggregated], &tally).unwrap().as_slice()).unwrap();

    match tallied {
        RadonTypes::Float(radon_float) => {
            assert!((radon_float.value() - 123f64).abs() < std::f64::EPSILON)
        }
        err => panic!("Error in run_retrieval: {:?}", err),
    }
}
