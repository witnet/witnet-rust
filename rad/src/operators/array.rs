use crate::error::RadError;
use crate::reducers::{self, RadonReducers};
use crate::script::{execute_radon_script, unpack_radon_call};
use crate::types::{array::RadonArray, RadonType, RadonTypes};
use num_traits::FromPrimitive;
use rmpv::Value;

pub fn reduce(input: &RadonArray, args: &[Value]) -> Result<RadonTypes, RadError> {
    let error = || RadError::WrongArguments {
        input_type: "RadonArray".to_string(),
        operator: "Reduce".to_string(),
        args: args.to_vec(),
    };

    let reducer_integer = args.first().ok_or_else(error)?.as_i64().ok_or_else(error)?;
    let reducer_code = RadonReducers::from_i64(reducer_integer).ok_or_else(error)?;

    reducers::reduce(input, reducer_code)
}

pub fn get(input: &RadonArray, args: &[Value]) -> Result<RadonTypes, RadError> {
    let key = args.first().map(|ref value| value.as_u64()).unwrap_or(None);
    match key {
        Some(key_str) => match input.value().get(key_str as usize) {
            Some(value) => Ok(value.clone()),
            None => Err(RadError::MapKeyNotFound {
                key: key_str.to_string(),
            }),
        },
        None => Err(RadError::WrongArguments {
            input_type: input.to_string(),
            operator: "Get".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn map(input: &RadonArray, args: &[Value]) -> Result<RadonTypes, RadError> {
    let mut subscript = vec![];
    for arg in args {
        subscript.push(unpack_radon_call(arg).unwrap())
    }

    let mut result = vec![];
    for item in input.value() {
        result.push(execute_radon_script(item, subscript.as_slice()).unwrap());
    }

    Ok(RadonArray::from(result).into())
}

#[test]
fn test_reduce_no_args() {
    use crate::types::float::RadonFloat;

    let input = &RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let args = &[];

    let result = reduce(input, args);

    assert_eq!(
        &result.unwrap_err().to_string(),
        "Wrong `RadonArray::Reduce()` arguments: `[]`"
    );
}

#[test]
fn test_reduce_wrong_args() {
    use crate::types::float::RadonFloat;

    let input = &RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let args = &[Value::from("wrong")]; // This is RadonReducers::AverageMean

    let result = reduce(input, args);

    assert_eq!(
        &result.unwrap_err().to_string(),
        "Wrong `RadonArray::Reduce()` arguments: `[String(Utf8String { s: Ok(\"wrong\") })]`"
    );
}

#[test]
fn test_reduce_unknown_reducer() {
    use crate::types::float::RadonFloat;

    let input = &RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let args = &[Value::from(-1)]; // This doesn't match any reducer code in RadonReducers

    let result = reduce(input, args);

    assert_eq!(
        &result.unwrap_err().to_string(),
        "Wrong `RadonArray::Reduce()` arguments: `[Integer(NegInt(-1))]`"
    );
}

#[test]
fn test_reduce_average_mean_float() {
    use crate::types::float::RadonFloat;

    let input = &RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let args = &[Value::from(0x03)]; // This is RadonReducers::AverageMean
    let expected = RadonTypes::from(RadonFloat::from(1.5f64));

    let output = reduce(input, args).unwrap();

    assert_eq!(output, expected);
}
