use std::{borrow::ToOwned, convert::TryFrom};

use serde_cbor::value::{from_value, Value};

use crate::{
    error::RadError,
    types::{
        boolean::RadonBoolean, float::RadonFloat, integer::RadonInteger, string::RadonString,
        RadonType,
    },
};

pub fn absolute(input: &RadonFloat) -> RadonFloat {
    RadonFloat::from(input.value().abs())
}

pub fn to_string(input: RadonFloat) -> Result<RadonString, RadError> {
    RadonString::try_from(Value::Text(input.value().to_string()))
}

// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(clippy::cast_possible_truncation)]
pub fn ceiling(input: &RadonFloat) -> RadonInteger {
    RadonInteger::from(input.value().ceil() as i128)
}

pub fn multiply(input: &RadonFloat, args: &[Value]) -> Result<RadonFloat, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonFloat::radon_type_name(),
        operator: "Multiply".to_string(),
        args: args.to_vec(),
    };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let multiplier = from_value::<f64>(arg).map_err(|_| wrong_args())?;
    Ok(RadonFloat::from(input.value() * multiplier))
}

pub fn greater_than(input: &RadonFloat, args: &[Value]) -> Result<RadonBoolean, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonFloat::radon_type_name(),
        operator: "GreaterThan".to_string(),
        args: args.to_vec(),
    };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let other = from_value::<f64>(arg).map_err(|_| wrong_args())?;
    Ok(RadonBoolean::from(input.value() > other))
}

pub fn less_than(input: &RadonFloat, args: &[Value]) -> Result<RadonBoolean, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonFloat::radon_type_name(),
        operator: "LessThan".to_string(),
        args: args.to_vec(),
    };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let other = from_value::<f64>(arg).map_err(|_| wrong_args())?;
    Ok(RadonBoolean::from(input.value() < other))
}

pub fn modulo(input: &RadonFloat, args: &[Value]) -> Result<RadonFloat, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonFloat::radon_type_name(),
        operator: "Modulo".to_string(),
        args: args.to_vec(),
    };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let modulo = from_value::<f64>(arg).map_err(|_| wrong_args())?;
    Ok(RadonFloat::from(input.value() % modulo))
}

pub fn negate(input: &RadonFloat) -> RadonFloat {
    RadonFloat::from(-input.value())
}

pub fn power(input: &RadonFloat, args: &[Value]) -> Result<RadonFloat, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonFloat::radon_type_name(),
        operator: "Power".to_string(),
        args: args.to_vec(),
    };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let exp = from_value::<f64>(arg).map_err(|_| wrong_args())?;

    Ok(RadonFloat::from(input.value().powf(exp)))
}

// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(clippy::cast_possible_truncation)]
pub fn floor(input: &RadonFloat) -> RadonInteger {
    RadonInteger::from(input.value().floor() as i128)
}

// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(clippy::cast_possible_truncation)]
pub fn round(input: &RadonFloat) -> RadonInteger {
    RadonInteger::from(input.value().round() as i128)
}

// No safe cast function from a float to integer yet, but this may just be fine since we are truncating anyway
#[allow(clippy::cast_possible_truncation)]
pub fn truncate(input: &RadonFloat) -> RadonInteger {
    RadonInteger::from(input.value().trunc() as i128)
}

#[test]
fn test_float_absolute() {
    let positive_integer = RadonFloat::from(10.0);
    let negative_integer = RadonFloat::from(-10.0);

    assert_eq!(absolute(&positive_integer), positive_integer);
    assert_eq!(absolute(&negative_integer), positive_integer);
}

#[test]
fn test_float_to_string() {
    let rad_int = RadonFloat::from(10.2);
    let rad_string: RadonString = RadonString::from("10.2");

    assert_eq!(to_string(rad_int).unwrap(), rad_string);
}

#[test]
fn test_float_multiply() {
    let rad_int = RadonFloat::from(10.0);
    let value = Value::Float(3.0);

    assert_eq!(
        multiply(&rad_int, &[value]).unwrap(),
        RadonFloat::from(30.0)
    );
}

#[test]
fn test_float_greater() {
    let rad_int = RadonFloat::from(10.0);
    let value = Value::Float(9.9);
    let value2 = Value::Float(10.0);
    let value3 = Value::Float(10.1);

    assert_eq!(
        greater_than(&rad_int, &[value]).unwrap(),
        RadonBoolean::from(true)
    );
    assert_eq!(
        greater_than(&rad_int, &[value2]).unwrap(),
        RadonBoolean::from(false)
    );
    assert_eq!(
        greater_than(&rad_int, &[value3]).unwrap(),
        RadonBoolean::from(false)
    );
}

#[test]
fn test_float_less() {
    let rad_int = RadonFloat::from(10.0);
    let value = Value::Float(9.9);
    let value2 = Value::Float(10.0);
    let value3 = Value::Float(10.1);

    assert_eq!(
        less_than(&rad_int, &[value]).unwrap(),
        RadonBoolean::from(false)
    );
    assert_eq!(
        less_than(&rad_int, &[value2]).unwrap(),
        RadonBoolean::from(false)
    );
    assert_eq!(
        less_than(&rad_int, &[value3]).unwrap(),
        RadonBoolean::from(true)
    );
}

#[test]
fn test_float_negate() {
    let positive_integer = RadonFloat::from(10.0);
    let negative_integer = RadonFloat::from(-10.0);

    assert_eq!(negate(&positive_integer), negative_integer);
    assert_eq!(negate(&negative_integer), positive_integer);
}

#[test]
fn test_float_modulo() {
    assert_eq!(
        modulo(&RadonFloat::from(5.0), &[Value::Float(3.0)]).unwrap(),
        RadonFloat::from(2.0)
    );
    assert_eq!(
        modulo(&RadonFloat::from(5.0), &[Value::Float(-3.0)]).unwrap(),
        RadonFloat::from(2.0)
    );
    assert_eq!(
        modulo(&RadonFloat::from(-5.0), &[Value::Float(3.0)]).unwrap(),
        RadonFloat::from(-2.0)
    );
    assert_eq!(
        modulo(&RadonFloat::from(-5.0), &[Value::Float(-3.0)]).unwrap(),
        RadonFloat::from(-2.0)
    );
}

#[test]
fn test_float_power() {
    let rad_int = RadonFloat::from(10.0);
    let value = Value::Float(3.0);

    assert_eq!(power(&rad_int, &[value]).unwrap(), RadonFloat::from(1000.0));
}

#[test]
fn test_float_ceiling() {
    let float1 = RadonFloat::from(10.01);
    let float2 = RadonFloat::from(11.0);
    let float3 = RadonFloat::from(-10.99);

    assert_eq!(ceiling(&float1), RadonInteger::from(11));
    assert_eq!(ceiling(&float2), RadonInteger::from(11));
    assert_eq!(ceiling(&float3), RadonInteger::from(-10));
}

#[test]
fn test_float_floor() {
    let float1 = RadonFloat::from(10.0);
    let float2 = RadonFloat::from(10.99);
    let float3 = RadonFloat::from(-10.01);

    assert_eq!(floor(&float1), RadonInteger::from(10));
    assert_eq!(floor(&float2), RadonInteger::from(10));
    assert_eq!(floor(&float3), RadonInteger::from(-11));
}

#[test]
fn test_float_round() {
    let float1 = RadonFloat::from(10.49);
    let float2 = RadonFloat::from(10.5);
    let float3 = RadonFloat::from(10.51);

    assert_eq!(round(&float1), RadonInteger::from(10));
    assert_eq!(round(&float2), RadonInteger::from(11));
    assert_eq!(round(&float3), RadonInteger::from(11));
}

#[test]
fn test_float_trunc() {
    let float1 = RadonFloat::from(10.0);
    let float2 = RadonFloat::from(10.99);
    let float3 = RadonFloat::from(-10.01);

    assert_eq!(truncate(&float1), RadonInteger::from(10));
    assert_eq!(truncate(&float2), RadonInteger::from(10));
    assert_eq!(truncate(&float3), RadonInteger::from(-10));
}
