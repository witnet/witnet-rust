use std::convert::TryFrom;

use witnet_data_structures::{
    mainnet_validations::current_active_wips,
    radon_error::RadonError,
    radon_report::{RadonReport, ReportContext},
};
use witnet_rad::{
    conditions::*,
    error::RadError,
    types::{array::RadonArray, float::RadonFloat, integer::RadonInteger, RadonTypes},
};

#[test]
fn test_tally_precondition_clause_3_ints_vs_1_float() {
    let rad_int = RadonTypes::Integer(RadonInteger::from(1));
    let rad_float = RadonTypes::Float(RadonFloat::from(1));

    let rad_rep_int = RadonReport::from_result(Ok(rad_int.clone()), &ReportContext::default());
    let rad_rep_float = RadonReport::from_result(Ok(rad_float), &ReportContext::default());

    let v = vec![
        rad_rep_int.clone(),
        rad_rep_int.clone(),
        rad_rep_int,
        rad_rep_float,
    ];
    let tally_precondition_clause_result =
        evaluate_tally_precondition_clause(v, 0.70, 4, &current_active_wips()).unwrap();

    if let TallyPreconditionClauseResult::MajorityOfValues {
        values,
        liars,
        errors,
    } = tally_precondition_clause_result
    {
        assert_eq!(values, vec![rad_int.clone(), rad_int.clone(), rad_int]);
        assert_eq!(liars, vec![false, false, false, true]);
        assert_eq!(errors, vec![false, false, false, false]);
    } else {
        panic!(
            "The result of the tally precondition clause was not `MajorityOfValues`. It was: {:?}",
            tally_precondition_clause_result
        );
    }
}

#[test]
fn test_tally_precondition_clause_full_consensus() {
    let rad_int = RadonTypes::Integer(RadonInteger::from(1));

    let rad_rep_int = RadonReport::from_result(Ok(rad_int.clone()), &ReportContext::default());

    let v = vec![rad_rep_int.clone(), rad_rep_int];
    let tally_precondition_clause_result =
        evaluate_tally_precondition_clause(v, 0.99, 2, &current_active_wips()).unwrap();

    if let TallyPreconditionClauseResult::MajorityOfValues {
        values,
        liars,
        errors,
    } = tally_precondition_clause_result
    {
        assert_eq!(values, vec![rad_int.clone(), rad_int]);
        assert_eq!(liars, vec![false, false]);
        assert_eq!(errors, vec![false, false]);
    } else {
        panic!(
            "The result of the tally precondition clause was not `MajorityOfValues`. It was: {:?}",
            tally_precondition_clause_result
        );
    }
}

#[test]
fn test_tally_precondition_clause_exact_consensus() {
    let rad_int = RadonTypes::Integer(RadonInteger::from(1));

    let rad_rep_int = RadonReport::from_result(Ok(rad_int.clone()), &ReportContext::default());

    let v = vec![rad_rep_int.clone(), rad_rep_int];
    let tally_precondition_clause_result =
        evaluate_tally_precondition_clause(v, 1., 2, &current_active_wips()).unwrap();

    if let TallyPreconditionClauseResult::MajorityOfValues {
        values,
        liars,
        errors,
    } = tally_precondition_clause_result
    {
        assert_eq!(values, vec![rad_int.clone(), rad_int]);
        assert_eq!(liars, vec![false, false]);
        assert_eq!(errors, vec![false, false]);
    } else {
        panic!(
            "The result of the tally precondition clause was not `MajorityOfValues`. It was: {:?}",
            tally_precondition_clause_result
        );
    }
}

#[test]
fn test_tally_precondition_clause_3_ints_vs_1_error() {
    let rad_int = RadonTypes::Integer(RadonInteger::from(1));
    let rad_err = RadError::HttpStatus { status_code: 404 };

    let rad_rep_int = RadonReport::from_result(Ok(rad_int.clone()), &ReportContext::default());
    let rad_rep_err = RadonReport::from_result(Err(rad_err), &ReportContext::default());

    let v = vec![
        rad_rep_int.clone(),
        rad_rep_err,
        rad_rep_int.clone(),
        rad_rep_int,
    ];
    let tally_precondition_clause_result =
        evaluate_tally_precondition_clause(v, 0.70, 4, &current_active_wips()).unwrap();

    if let TallyPreconditionClauseResult::MajorityOfValues {
        values,
        liars,
        errors,
    } = tally_precondition_clause_result
    {
        assert_eq!(values, vec![rad_int.clone(), rad_int.clone(), rad_int]);
        assert_eq!(liars, vec![false, true, false, false]);
        assert_eq!(errors, vec![false, true, false, false]);
    } else {
        panic!(
            "The result of the tally precondition clause was not `MajorityOfValues`. It was: {:?}",
            tally_precondition_clause_result
        );
    }
}

#[test]
fn test_tally_precondition_clause_majority_of_errors() {
    let rad_int = RadonTypes::Integer(RadonInteger::from(1));
    let rad_err = RadonError::try_from(RadError::HttpStatus { status_code: 0 }).unwrap();

    let rad_rep_int = RadonReport::from_result(Ok(rad_int), &ReportContext::default());
    let rad_rep_err = RadonReport::from_result(
        Ok(RadonTypes::RadonError(rad_err.clone())),
        &ReportContext::default(),
    );

    let v = vec![
        rad_rep_err.clone(),
        rad_rep_err.clone(),
        rad_rep_err,
        rad_rep_int,
    ];
    let tally_precondition_clause_result =
        evaluate_tally_precondition_clause(v, 0.70, 4, &current_active_wips()).unwrap();

    if let TallyPreconditionClauseResult::MajorityOfErrors { errors_mode } =
        tally_precondition_clause_result
    {
        assert_eq!(errors_mode, rad_err);
    } else {
        panic!(
            "The result of the tally precondition clause was not `MajorityOfErrors`. It was: {:?}",
            tally_precondition_clause_result
        );
    }
}

#[test]
fn test_tally_precondition_clause_mode_tie() {
    let rad_int = RadonTypes::Integer(RadonInteger::from(1));
    let rad_float = RadonTypes::Float(RadonFloat::from(1));

    let rad_rep_int = RadonReport::from_result(Ok(rad_int), &ReportContext::default());
    let rad_rep_float = RadonReport::from_result(Ok(rad_float), &ReportContext::default());

    let v = vec![
        rad_rep_float.clone(),
        rad_rep_int.clone(),
        rad_rep_float,
        rad_rep_int,
    ];
    let out =
        evaluate_tally_precondition_clause(v.clone(), 0.49, 4, &current_active_wips()).unwrap_err();

    assert_eq!(
        out,
        RadError::ModeTie {
            values: RadonArray::from(
                v.into_iter()
                    .map(RadonReport::into_inner)
                    .collect::<Vec<RadonTypes>>()
            ),
            max_count: 2,
        }
    );
}

#[test]
fn test_tally_precondition_clause_3_errors_vs_2_ints_and_2_floats() {
    let rad_int = RadonTypes::Integer(RadonInteger::from(1));
    let rad_float = RadonTypes::Float(RadonFloat::from(1));
    let rad_err = RadonError::try_from(RadError::HttpStatus { status_code: 0 }).unwrap();

    let rad_rep_int = RadonReport::from_result(Ok(rad_int), &ReportContext::default());
    let rad_rep_float = RadonReport::from_result(Ok(rad_float), &ReportContext::default());
    let rad_rep_err = RadonReport::from_result(
        Ok(RadonTypes::RadonError(rad_err.clone())),
        &ReportContext::default(),
    );

    let v = vec![
        rad_rep_err.clone(),
        rad_rep_err.clone(),
        rad_rep_err,
        rad_rep_float.clone(),
        rad_rep_int.clone(),
        rad_rep_float,
        rad_rep_int,
    ];
    let tally_precondition_clause_result =
        evaluate_tally_precondition_clause(v, 0.40, 7, &current_active_wips()).unwrap();

    if let TallyPreconditionClauseResult::MajorityOfErrors { errors_mode } =
        tally_precondition_clause_result
    {
        assert_eq!(errors_mode, rad_err);
    } else {
        panic!(
            "The result of the tally precondition clause was not `MajorityOfErrors`. It was: {:?}",
            tally_precondition_clause_result
        );
    }
}

#[test]
fn test_tally_precondition_clause_no_commits() {
    let v = vec![];
    let out = evaluate_tally_precondition_clause(v, 0.51, 0, &current_active_wips()).unwrap_err();

    assert_eq!(out, RadError::InsufficientCommits);
}

#[test]
fn test_tally_precondition_clause_no_reveals() {
    let v = vec![];
    let out = evaluate_tally_precondition_clause(v, 0.51, 1, &current_active_wips()).unwrap_err();

    assert_eq!(out, RadError::NoReveals);
}

#[test]
fn test_tally_precondition_clause_all_errors() {
    let rad_err = RadonError::try_from(RadError::HttpStatus { status_code: 0 }).unwrap();
    let rad_rep_err = RadonReport::from_result(
        Ok(RadonTypes::RadonError(rad_err.clone())),
        &ReportContext::default(),
    );

    let v = vec![
        rad_rep_err.clone(),
        rad_rep_err.clone(),
        rad_rep_err.clone(),
        rad_rep_err,
    ];
    let tally_precondition_clause_result =
        evaluate_tally_precondition_clause(v, 0.51, 4, &current_active_wips()).unwrap();

    if let TallyPreconditionClauseResult::MajorityOfErrors { errors_mode } =
        tally_precondition_clause_result
    {
        assert_eq!(errors_mode, rad_err);
    } else {
        panic!(
            "The result of the tally precondition clause was not `MajorityOfErrors`. It was: {:?}",
            tally_precondition_clause_result
        );
    }
}

#[test]
fn test_tally_precondition_clause_insufficient_consensus() {
    let rad_int = RadonTypes::Integer(RadonInteger::from(1));
    let rad_float = RadonTypes::Float(RadonFloat::from(1));

    let rad_rep_int = RadonReport::from_result(Ok(rad_int), &ReportContext::default());
    let rad_rep_float = RadonReport::from_result(Ok(rad_float), &ReportContext::default());

    let v = vec![
        rad_rep_float.clone(),
        rad_rep_int.clone(),
        rad_rep_float,
        rad_rep_int,
    ];
    let out = evaluate_tally_precondition_clause(v, 0.51, 4, &current_active_wips()).unwrap_err();

    assert_eq!(
        out,
        RadError::InsufficientConsensus {
            achieved: 0.5,
            required: 0.51,
        }
    );
}

#[test]
fn test_tally_precondition_clause_errors_insufficient_consensus() {
    // Two revealers that report two different errors result in `InsufficientConsensus`
    // because there is only 50% consensus (1/2)
    let rad_err1 = RadonError::try_from(RadError::HttpStatus { status_code: 0 }).unwrap();
    let rad_err2 = RadonError::try_from(RadError::RetrieveTimeout).unwrap();
    let rad_rep_err1 = RadonReport::from_result(
        Ok(RadonTypes::RadonError(rad_err1)),
        &ReportContext::default(),
    );
    let rad_rep_err2 = RadonReport::from_result(
        Ok(RadonTypes::RadonError(rad_err2)),
        &ReportContext::default(),
    );

    let v = vec![rad_rep_err1, rad_rep_err2];
    let out = evaluate_tally_precondition_clause(v, 0.51, 2, &current_active_wips()).unwrap_err();

    assert_eq!(
        out,
        RadError::InsufficientConsensus {
            achieved: 0.5,
            required: 0.51,
        }
    );
}

#[test]
fn test_tally_precondition_clause_errors_mode_tie() {
    // Two revealers that report two different errors when min_consensus is below 50%
    // result in RadError::ModeTie
    let rad_err1 = RadonError::try_from(RadError::HttpStatus { status_code: 0 }).unwrap();
    let rad_err2 = RadonError::try_from(RadError::RetrieveTimeout).unwrap();
    let rad_rep_err1 = RadonReport::from_result(
        Ok(RadonTypes::RadonError(rad_err1)),
        &ReportContext::default(),
    );
    let rad_rep_err2 = RadonReport::from_result(
        Ok(RadonTypes::RadonError(rad_err2)),
        &ReportContext::default(),
    );

    let v = vec![rad_rep_err1, rad_rep_err2];
    let out =
        evaluate_tally_precondition_clause(v.clone(), 0.49, 2, &current_active_wips()).unwrap_err();

    assert_eq!(
        out,
        RadError::ModeTie {
            values: RadonArray::from(
                v.into_iter()
                    .map(RadonReport::into_inner)
                    .collect::<Vec<RadonTypes>>()
            ),
            max_count: 1,
        }
    );
}
