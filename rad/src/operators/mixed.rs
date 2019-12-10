use std::convert::TryFrom;

use crate::{
    error::RadError,
    types::{
        array::RadonArray, boolean::RadonBoolean, float::RadonFloat, integer::RadonInteger,
        map::RadonMap, mixed::RadonMixed, string::RadonString, RadonType,
    },
};

pub fn to_float(input: RadonMixed) -> Result<RadonFloat, RadError> {
    RadonFloat::try_from(input.value())
}

pub fn to_int(input: RadonMixed) -> Result<RadonInteger, RadError> {
    RadonInteger::try_from(input.value())
}

pub fn to_map(input: RadonMixed) -> Result<RadonMap, RadError> {
    RadonMap::try_from(input.value())
}

pub fn to_array(input: RadonMixed) -> Result<RadonArray, RadError> {
    RadonArray::try_from(input.value())
}

pub fn to_bool(input: RadonMixed) -> Result<RadonBoolean, RadError> {
    RadonBoolean::try_from(input.value())
}

pub fn to_string(input: RadonMixed) -> Result<RadonString, RadError> {
    RadonString::try_from(input.value())
}

#[test]
fn test_as_float() {
    use serde_cbor::value::Value;

    let radon_float = RadonFloat::from(std::f64::consts::PI);
    let radon_mixed = RadonMixed::from(Value::try_from(std::f64::consts::PI).unwrap());
    assert_eq!(to_float(radon_mixed).unwrap(), radon_float);

    let radon_mixed_error =
        RadonMixed::from(Value::try_from(String::from("Hello world!")).unwrap());
    assert_eq!(
        &to_float(radon_mixed_error).unwrap_err().to_string(),
        "Failed to convert string to float with error message: invalid float literal"
    );
}

#[test]
fn test_as_integer() {
    use serde_cbor::value::Value;

    let radon_int = RadonInteger::from(10);
    let radon_mixed = RadonMixed::from(Value::try_from(10).unwrap());
    assert_eq!(to_int(radon_mixed).unwrap(), radon_int);

    let radon_mixed_error =
        RadonMixed::from(Value::try_from(String::from("Hello world!")).unwrap());
    assert_eq!(
        &to_int(radon_mixed_error).unwrap_err().to_string(),
        "Failed to convert string to int with error message: invalid digit found in string"
    );
}

#[test]
fn test_as_bool() {
    use serde_cbor::value::Value;

    let radon_bool = RadonBoolean::from(false);
    let radon_mixed = RadonMixed::from(Value::try_from(false).unwrap());
    assert_eq!(to_bool(radon_mixed).unwrap(), radon_bool);

    let radon_mixed_error =
        RadonMixed::from(Value::try_from(String::from("Hello world!")).unwrap());
    assert_eq!(
        &to_bool(radon_mixed_error).unwrap_err().to_string(),
        "Failed to decode RadonBoolean from cbor::value::Value"
    );
}

#[test]
fn test_as_string() {
    use serde_cbor::value::Value;

    let radon_string = RadonString::from("Hello world!");
    let radon_mixed = RadonMixed::from(Value::try_from(String::from("Hello world!")).unwrap());
    assert_eq!(to_string(radon_mixed).unwrap(), radon_string);

    let radon_mixed_error = RadonMixed::from(Value::try_from(std::f64::consts::PI).unwrap());
    assert_eq!(
        &to_string(radon_mixed_error).unwrap_err().to_string(),
        "Failed to decode RadonString from serde_cbor::value::Value"
    );
}
