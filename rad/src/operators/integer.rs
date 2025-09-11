use std::{borrow::ToOwned, convert::TryFrom};

use serde_cbor::value::{Value, from_value};

use crate::{
    error::RadError,
    operators::decode_single_arg,
    types::{
        RadonType,
        boolean::RadonBoolean,
        bytes::{RadonBytes, RadonBytesEndianness},
        float::RadonFloat,
        integer::RadonInteger,
        string::RadonString,
    },
};

pub fn absolute(input: &RadonInteger) -> Result<RadonInteger, RadError> {
    let result = input.value().checked_abs();

    if let Some(result) = result {
        Ok(RadonInteger::from(result))
    } else {
        Err(RadError::Overflow)
    }
}

pub fn to_bytes(input: &RadonInteger, args: &Option<Vec<Value>>) -> Result<RadonBytes, RadError> {
    let endianness =
        decode_single_arg::<RadonInteger, u8, RadonBytesEndianness, _, _>(args, "ToBytes")?;
    let encoder = if let RadonBytesEndianness::Big = endianness {
        i128::to_be_bytes
    } else {
        i128::to_le_bytes
    };

    let bytes = encoder(input.value()).to_vec();

    Ok(RadonBytes::from(bytes))
}

pub fn to_float(input: &RadonInteger) -> Result<RadonFloat, RadError> {
    RadonFloat::try_from(Value::Integer(input.value()))
}

pub fn to_string(input: &RadonInteger) -> Result<RadonString, RadError> {
    RadonString::try_from(Value::Text(input.value().to_string()))
}

pub fn multiply(input: &RadonInteger, args: &[Value]) -> Result<RadonInteger, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonInteger::radon_type_name(),
        operator: "Multiply".to_string(),
        args: args.to_vec(),
    };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let multiplier = from_value::<i128>(arg).map_err(|_| wrong_args())?;
    let result = input.value().checked_mul(multiplier);

    if let Some(result) = result {
        Ok(RadonInteger::from(result))
    } else {
        Err(RadError::Overflow)
    }
}

pub fn greater_than(input: &RadonInteger, args: &[Value]) -> Result<RadonBoolean, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonInteger::radon_type_name(),
        operator: "GreaterThan".to_string(),
        args: args.to_vec(),
    };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let other = from_value::<i128>(arg).map_err(|_| wrong_args())?;
    Ok(RadonBoolean::from(input.value() > other))
}

pub fn less_than(input: &RadonInteger, args: &[Value]) -> Result<RadonBoolean, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonInteger::radon_type_name(),
        operator: "LessThan".to_string(),
        args: args.to_vec(),
    };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let other = from_value::<i128>(arg).map_err(|_| wrong_args())?;
    Ok(RadonBoolean::from(input.value() < other))
}

pub fn modulo(input: &RadonInteger, args: &[Value]) -> Result<RadonInteger, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonInteger::radon_type_name(),
        operator: "Modulo".to_string(),
        args: args.to_vec(),
    };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let modulo = from_value::<i128>(arg).map_err(|_| wrong_args())?;

    match input.value().checked_rem(modulo) {
        Some(x) => Ok(RadonInteger::from(x)),
        None => Err(RadError::Overflow),
    }
}

pub fn negate(input: &RadonInteger) -> Result<RadonInteger, RadError> {
    let result = input.value().checked_neg();

    if let Some(result) = result {
        Ok(RadonInteger::from(result))
    } else {
        Err(RadError::Overflow)
    }
}

pub fn power(input: &RadonInteger, args: &[Value]) -> Result<RadonInteger, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonInteger::radon_type_name(),
        operator: "Power".to_string(),
        args: args.to_vec(),
    };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let exp = from_value::<u32>(arg).map_err(|_| wrong_args())?;
    let result = input.value().checked_pow(exp);

    if let Some(result) = result {
        Ok(RadonInteger::from(result))
    } else {
        Err(RadError::Overflow)
    }
}

#[test]
fn test_integer_absolute() {
    let positive_integer = RadonInteger::from(10);
    let negative_integer = RadonInteger::from(-10);

    assert_eq!(absolute(&positive_integer).unwrap(), positive_integer);
    assert_eq!(absolute(&negative_integer).unwrap(), positive_integer);
    assert_eq!(
        absolute(&RadonInteger::from(i128::MIN))
            .unwrap_err()
            .to_string(),
        "Overflow error".to_string(),
    );
}

#[test]
fn test_integer_to_bytes() {
    let input = RadonInteger::from(i128::MAX);
    let expected_big = Ok(RadonBytes::from(vec![
        0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF,
    ]));
    let expected_little = Ok(RadonBytes::from(vec![
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0x7F,
    ]));

    // No arguments, default to big endian
    let args = None;
    let output = to_bytes(&input, &args);
    assert_eq!(output, expected_big);

    // Empty arguments, default to big endian
    let args = Some(vec![]);
    let output = to_bytes(&input, &args);
    assert_eq!(output, expected_big);

    // Big endian
    let args = Some(vec![Value::Integer(i128::from(
        RadonBytesEndianness::Big as u8,
    ))]);
    let output = to_bytes(&input, &args);
    assert_eq!(output, expected_big);

    // Little endian
    let args = Some(vec![Value::Integer(i128::from(
        RadonBytesEndianness::Little as u8,
    ))]);
    let output = to_bytes(&input, &args);
    assert_eq!(output, expected_little);

    // Any non-little is a big
    let args = Some(vec![Value::Integer(123)]);
    let output = to_bytes(&input, &args);
    assert_eq!(output, expected_big);

    // Invalid argument semantics, fail
    let args = vec![Value::Integer(123456)];
    let output = to_bytes(&input, &Some(args.clone()));
    let expected = Err(RadError::WrongArguments {
        input_type: "RadonInteger",
        operator: "ToBytes".to_string(),
        args,
    });
    assert_eq!(output, expected);

    // Invalid argument type, fail
    let args = vec![Value::Text(String::from("whatever"))];
    let output = to_bytes(&input, &Some(args.clone()));
    let expected = Err(RadError::WrongArguments {
        input_type: "RadonInteger",
        operator: "ToBytes".to_string(),
        args,
    });
    assert_eq!(output, expected);
}

#[test]
fn test_integer_to_float() {
    let rad_int = RadonInteger::from(10);
    let rad_float = RadonFloat::from(10.0);

    assert_eq!(to_float(&rad_int).unwrap(), rad_float);
}

#[test]
fn test_integer_to_string() {
    let rad_int = RadonInteger::from(10);
    let rad_string: RadonString = RadonString::from("10");

    assert_eq!(to_string(&rad_int).unwrap(), rad_string);
}

#[test]
fn test_integer_multiply() {
    let rad_int = RadonInteger::from(10);
    let value = Value::Integer(3);

    assert_eq!(
        multiply(&rad_int, &[value]).unwrap(),
        RadonInteger::from(30)
    );

    let value = Value::Integer(3);
    assert_eq!(
        multiply(&RadonInteger::from(i128::MAX), &[value])
            .unwrap_err()
            .to_string(),
        "Overflow error".to_string(),
    );
}

#[test]
fn test_integer_greater() {
    let rad_int = RadonInteger::from(10);
    let value = Value::Integer(9);
    let value2 = Value::Integer(10);
    let value3 = Value::Integer(11);

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
fn test_integer_less() {
    let rad_int = RadonInteger::from(10);
    let value = Value::Integer(9);
    let value2 = Value::Integer(10);
    let value3 = Value::Integer(11);

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
fn test_integer_negate() {
    let positive_integer = RadonInteger::from(10);
    let negative_integer = RadonInteger::from(-10);

    assert_eq!(negate(&positive_integer).unwrap(), negative_integer);
    assert_eq!(negate(&negative_integer).unwrap(), positive_integer);

    assert_eq!(
        negate(&RadonInteger::from(i128::MIN))
            .unwrap_err()
            .to_string(),
        "Overflow error".to_string(),
    );
}

#[test]
fn test_integer_modulo() {
    assert_eq!(
        modulo(&RadonInteger::from(5), &[Value::Integer(3)]).unwrap(),
        RadonInteger::from(2)
    );
    assert_eq!(
        modulo(&RadonInteger::from(5), &[Value::Integer(-3)]).unwrap(),
        RadonInteger::from(2)
    );
    assert_eq!(
        modulo(&RadonInteger::from(-5), &[Value::Integer(3)]).unwrap(),
        RadonInteger::from(-2)
    );
    assert_eq!(
        modulo(&RadonInteger::from(-5), &[Value::Integer(-3)]).unwrap(),
        RadonInteger::from(-2)
    );

    assert_eq!(
        modulo(&RadonInteger::from(5), &[Value::Integer(0)]).unwrap_err(),
        RadError::Overflow,
    );

    assert_eq!(
        modulo(&RadonInteger::from(i128::MIN), &[Value::Integer(-1)]).unwrap_err(),
        RadError::Overflow,
    );
}

#[test]
fn test_integer_power() {
    let rad_int = RadonInteger::from(10);
    let value = Value::Integer(3);

    assert_eq!(power(&rad_int, &[value]).unwrap(), RadonInteger::from(1000));

    let rad_int = RadonInteger::from(i128::MAX);
    let value = Value::Integer(3);
    assert_eq!(
        power(&rad_int, &[value]).unwrap_err().to_string(),
        "Overflow error".to_string(),
    );
}
