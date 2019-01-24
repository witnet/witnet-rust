use crate::error::*;
use crate::reducers::{self, RadonReducers};
use crate::types::{array::RadonArray, RadonTypes};

use num_traits::FromPrimitive;
use rmpv::Value;

pub fn reduce(input: &RadonArray, args: &[Value]) -> RadResult<RadonTypes> {
    let error = || {
        WitnetError::from(RadError::new(
            RadErrorKind::WrongReducerArguments,
            format!("Wrong RadonArray reducer arguments: {:?}", args),
        ))
    };

    let reducer_integer = args.first().ok_or_else(error)?.as_i64().ok_or_else(error)?;
    let reducer_code = RadonReducers::from_i64(reducer_integer).ok_or_else(error)?;

    reducers::reduce(input, reducer_code)
}

#[test]
fn test_reduce_no_args() {
    use crate::types::float::RadonFloat;

    let input = &RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let args = &[];

    let correctness = if let Err(err) = reduce(input, args) {
        err.inner().kind() == &RadErrorKind::WrongReducerArguments
    } else {
        false
    };

    assert!(correctness);
}

#[test]
fn test_reduce_wrong_args() {
    use crate::types::float::RadonFloat;

    let input = &RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let args = &[Value::from("wrong")]; // This is RadonReducers::AverageMean

    let correctness = if let Err(err) = reduce(input, args) {
        err.inner().kind() == &RadErrorKind::WrongReducerArguments
    } else {
        false
    };

    assert!(correctness);
}

#[test]
fn test_reduce_unknown_reducer() {
    use crate::types::float::RadonFloat;

    let input = &RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let args = &[Value::from(-1)]; // This doesn't match any reducer code in RadonReducers

    let correctness = if let Err(err) = reduce(input, args) {
        err.inner().kind() == &RadErrorKind::WrongReducerArguments
    } else {
        false
    };

    assert!(correctness);
}

#[test]
fn test_reduce_average_mean_float() {
    use crate::types::float::RadonFloat;

    let input = &RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let args = &[Value::from(0x20)]; // This is RadonReducers::AverageMean
    let expected = RadonTypes::from(RadonFloat::from(1.5f64));

    let output = reduce(input, args).unwrap();

    assert_eq!(output, expected);
}
