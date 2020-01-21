//! This file contains tests that check if given a list of reports (reveals), it is possible
//! to construct a tally without returning `RadonErrors::UnhandledIntercept`.
//! Currently, some of this tests use `assert_unhandled_error`, which means that they return
//! `RadonErrors::UnhandledIntercept` when they shouldn't.
//! The goal is to eventually remove `RadonErrors::UnhandledIntercept`, so the tests that use
//! `assert_unhandled_error` must be fixed and compare `tally_result` with the expected error.

use witnet_data_structures::{
    chain::{DataRequestOutput, RADTally},
    radon_error::{RadonError, RadonErrors},
    radon_report::{RadonReport, ReportContext},
};
use witnet_node::actors::{messages::RunTally, rad_manager::RadManager};
use witnet_rad::{
    error::RadError,
    reducers::RadonReducers,
    types::{integer::RadonInteger, string::RadonString, RadonTypes},
};
use witnet_validations::validations::validate_data_request_output;

use std::convert::TryFrom;

// `RunTally` builders
fn mode_tally() -> RunTally {
    RunTally {
        reports: vec![],
        script: RADTally {
            filters: vec![],
            reducer: RadonReducers::Mode as u32,
        },
        min_consensus_ratio: 0.51,
    }
}

fn mean_tally() -> RunTally {
    RunTally {
        reports: vec![],
        script: RADTally {
            filters: vec![],
            reducer: RadonReducers::AverageMean as u32,
        },
        min_consensus_ratio: 0.51,
    }
}

// Helper function to convert RadonTypes into RadonReport
fn reports(reports_vec: Vec<Result<RadonTypes, RadError>>) -> Vec<RadonReport<RadonTypes>> {
    reports_vec
        .into_iter()
        .map(|r| RadonReport::from_result(r, &ReportContext::default()))
        .collect()
}

// This function asserts that `tally_result` is an `UnhandledIntercept`
// The idea is to eventually remove `UnhandledIntercept`, so any tests that use this function
// should return other `RadonError`s
fn assert_unhandled_error(tally_result: RadonTypes) {
    match tally_result {
        RadonTypes::RadonError(radon_error) => {
            assert_eq!(
                radon_error.into_inner().try_into_error_code(),
                Ok(RadonErrors::UnhandledIntercept)
            );
        }
        tally_result => panic!("Expected `RadonError`, got `{:?}`", tally_result),
    }
}

#[test]
fn run_tally_no_reveals() {
    let mut rad = RadManager::default();
    let tally_result = rad
        .run_tally(RunTally {
            reports: reports(vec![]),
            ..mode_tally()
        })
        .into_inner();

    assert_eq!(
        tally_result,
        RadonTypes::RadonError(RadonError::try_from(RadError::NoReveals).unwrap())
    );
}

#[test]
fn run_tally_malformed_reveal() {
    let mut rad = RadManager::default();
    let tally_result = rad
        .run_tally(RunTally {
            reports: reports(vec![Err(RadError::MalformedReveal)]),
            ..mode_tally()
        })
        .into_inner();

    assert_eq!(
        tally_result,
        RadonTypes::RadonError(RadonError::try_from(RadError::MalformedReveal).unwrap())
    );
}

#[test]
fn run_tally_50_percent_consensus() {
    let mut rad = RadManager::default();
    let tally_result = rad
        .run_tally(RunTally {
            reports: reports(vec![
                Ok(RadonTypes::String(RadonString::from("a"))),
                Ok(RadonTypes::Integer(RadonInteger::from(0))),
            ]),
            ..mode_tally()
        })
        .into_inner();

    assert_eq!(
        tally_result,
        RadonTypes::RadonError(
            RadonError::try_from(RadError::InsufficientConsensus {
                achieved: 0.50,
                required: 0.51
            })
            .unwrap()
        )
    );
}

#[test]
fn run_tally_mode_tie_on_precondition() {
    // As long as there exists a validation that `min_consensus_ratio` for tally must be >= 0.51,
    // this test is redundant
    let valid_dro = || DataRequestOutput {
        min_consensus_percentage: 51,
        witness_reward: 1,
        witnesses: 1,
        ..DataRequestOutput::default()
    };

    assert!(validate_data_request_output(&DataRequestOutput {
        min_consensus_percentage: 49,
        ..valid_dro()
    })
    .is_err());
    assert!(validate_data_request_output(&DataRequestOutput {
        min_consensus_percentage: 50,
        ..valid_dro()
    })
    .is_err());
    assert_eq!(
        validate_data_request_output(&DataRequestOutput {
            min_consensus_percentage: 51,
            ..valid_dro()
        }),
        Ok(())
    );

    let mut rad = RadManager::default();
    let tally_result = rad
        .run_tally(RunTally {
            reports: reports(vec![
                Ok(RadonTypes::String(RadonString::from("a"))),
                Ok(RadonTypes::Integer(RadonInteger::from(0))),
            ]),
            // This error is only possible if min_consensus_ratio < 0.50
            min_consensus_ratio: 0.49,
            ..mode_tally()
        })
        .into_inner();

    assert_unhandled_error(tally_result);
}

#[test]
fn run_tally_mode_tie_majority_of_errors() {
    let mut rad = RadManager::default();
    let tally_result = rad
        .run_tally(RunTally {
            reports: reports(vec![
                Err(RadError::MalformedReveal),
                Err(RadError::HttpStatus { status_code: 404 }),
            ]),
            ..mode_tally()
        })
        .into_inner();

    assert_eq!(
        tally_result,
        RadonTypes::RadonError(
            RadonError::try_from(RadError::InsufficientConsensus {
                achieved: 0.50,
                required: 0.51,
            })
            .unwrap()
        )
    );
}

#[test]
fn run_tally_mode_tie_when_running() {
    let mut rad = RadManager::default();
    let tally_result = rad
        .run_tally(RunTally {
            reports: reports(vec![
                Ok(RadonTypes::String(RadonString::from("a"))),
                Ok(RadonTypes::String(RadonString::from("b"))),
            ]),
            ..mode_tally()
        })
        .into_inner();

    assert_unhandled_error(tally_result)
}

#[test]
fn run_tally_http_other() {
    let mut rad = RadManager::default();
    let tally_result = rad
        .run_tally(RunTally {
            reports: reports(vec![Err(RadError::HttpOther {
                message: "couldn't resolve host name".to_string(),
            })]),
            ..mode_tally()
        })
        .into_inner();

    assert_unhandled_error(tally_result)
}

#[test]
fn run_tally_unsupported_reducer() {
    // Run AverageMean reducer on an array of RadonString
    let mut rad = RadManager::default();
    let tally_result = rad
        .run_tally(RunTally {
            reports: reports(vec![Ok(RadonTypes::String(RadonString::from("a")))]),
            ..mean_tally()
        })
        .into_inner();

    assert_unhandled_error(tally_result)
}

#[test]
fn run_tally_map_key_not_found() {
    let mut rad = RadManager::default();
    let tally_result = rad
        .run_tally(RunTally {
            reports: reports(vec![Err(RadError::MapKeyNotFound {
                key: "a".to_string(),
            })]),
            ..mode_tally()
        })
        .into_inner();

    assert_unhandled_error(tally_result)
}

#[test]
fn run_tally_parse_int() {
    let mut rad = RadManager::default();
    let tally_result = rad
        .run_tally(RunTally {
            reports: reports(vec![Err(RadError::ParseInt {
                message: "invalid digit found in string".to_string(),
            })]),
            ..mode_tally()
        })
        .into_inner();

    assert_unhandled_error(tally_result)
}

#[test]
fn run_tally_decode_error() {
    let mut rad = RadManager::default();
    let tally_result = rad
        .run_tally(RunTally {
            reports: reports(vec![Err(RadError::Decode {
                from: "cbor::value::Value".to_string(),
                to: "RadonInteger".to_string(),
            })]),
            ..mode_tally()
        })
        .into_inner();

    assert_unhandled_error(tally_result)
}
