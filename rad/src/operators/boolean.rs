use serde_cbor::Value;
use std::convert::TryFrom;

use crate::{
    error::RadError,
    types::{boolean::RadonBoolean, string::RadonString, RadonType},
};

pub fn negate(input: &RadonBoolean) -> RadonBoolean {
    RadonBoolean::from(!input.value())
}

pub fn to_string(input: RadonBoolean) -> Result<RadonString, RadError> {
    RadonString::try_from(Value::Text(input.value().to_string()))
}

#[test]
fn test_boolean_negate() {
    let true_bool = RadonBoolean::from(true);
    let false_bool = RadonBoolean::from(false);

    assert_eq!(negate(&true_bool), false_bool);
    assert_eq!(negate(&false_bool), true_bool);
}

#[test]
fn test_boolean_to_string() {
    let rad_int = RadonBoolean::from(false);
    let rad_string: RadonString = RadonString::from("false");

    assert_eq!(to_string(rad_int).unwrap(), rad_string);
}
