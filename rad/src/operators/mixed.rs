use crate::error::RadError;
use crate::types::{
    array::RadonArray, float::RadonFloat, map::RadonMap, mixed::RadonMixed, RadonType,
};

use std::convert::TryFrom;

pub fn to_float(input: RadonMixed) -> Result<RadonFloat, RadError> {
    RadonFloat::try_from(input.value())
}

pub fn to_map(input: RadonMixed) -> Result<RadonMap, RadError> {
    RadonMap::try_from(input.value())
}
pub fn to_array(input: RadonMixed) -> Result<RadonArray, RadError> {
    RadonArray::try_from(input.value())
}

#[test]
fn test_as_float() {
    use rmpv::Value;

    let radon_float = RadonFloat::from(std::f64::consts::PI);
    let radon_mixed_error = RadonMixed::from(Value::from(String::from("Hello world!")));
    let radon_mixed = RadonMixed::from(Value::from(std::f64::consts::PI));

    assert_eq!(to_float(radon_mixed).unwrap(), radon_float);
    assert_eq!(
        &to_float(radon_mixed_error).unwrap_err().to_string(),
        "Failed to decode RadonFloat from rmpv::Value"
    );
}
