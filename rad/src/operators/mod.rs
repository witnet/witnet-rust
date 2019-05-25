// FIXME: https://github.com/rust-num/num-derive/issues/20
#![allow(clippy::useless_attribute)]

use crate::error::RadError;
use crate::script::RadonCall;
use crate::types::RadonTypes;

use num_derive::FromPrimitive;
use std::fmt;

pub mod array;
pub mod boolean;
pub mod float;
pub mod map;
pub mod mixed;
pub mod string;

#[derive(Debug, FromPrimitive, PartialEq)]
pub enum RadonOpCodes {
    /// Only for the sake of allowing catch-alls when matching
    Fail = -1,
    // Multi-type operator codes start at 0x00
    /// Identity operator code
    Identity = 0x00,
    //Equals = 0x01,
    /// Array::get, Map::get, Result::get
    Get = 0x01,
    // Boolean operator codes start at 0x10
    BooleanNegate = 0x11,
    // Integer operator codes start at 0x20
    // Float operator codes start at 0x30
    FloatGreaterThan = 0x32,
    FloatLessThan = 0x34,
    FloatMultiply = 0x36,
    // String operator codes start at 0x40
    /// Compute the hash of a string
    StringHash = 0x40,
    /// Parse Mixed from JSON string
    StringParseJson = 0x43,
    StringToFloat = 0x46,
    // Array operator codes start at 0x50
    ArrayGet = 0x54,
    ArrayMap = 0x55,
    ArrayReduce = 0x56,
    // Map operator codes start at 0x60
    MapGet = 0x61,
    /// Flatten a map into an Array containing only the values but not the keys
    MapValues = 0x63,
    // Mixed operator codes start at 0x70
    MixedToArray = 0x70,
    MixedToFloat = 0x72,
    MixedToMap = 0x74,
    // Result operator codes start at 0x80
}

impl fmt::Display for RadonOpCodes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub trait Operable {
    fn operate(self, call: &RadonCall) -> Result<RadonTypes, RadError>;
}

pub fn operate(input: RadonTypes, call: &RadonCall) -> Result<RadonTypes, RadError> {
    match input {
        RadonTypes::Array(radon_array) => radon_array.operate(call),
        RadonTypes::Boolean(radon_boolean) => radon_boolean.operate(call),
        RadonTypes::Float(radon_float) => radon_float.operate(call),
        RadonTypes::Map(radon_map) => radon_map.operate(call),
        RadonTypes::String(radon_string) => radon_string.operate(call),
        RadonTypes::Mixed(radon_mixed) => radon_mixed.operate(call),
    }
}

pub fn identity(input: RadonTypes) -> Result<RadonTypes, RadError> {
    Ok(input)
}

#[test]
pub fn test_identity() {
    use crate::types::string::RadonString;

    let input = RadonString::from("Hello world!").into();
    let expected = RadonString::from("Hello world!").into();
    let output = identity(input).unwrap();

    assert_eq!(output, expected);
}

#[test]
pub fn test_operate() {
    use crate::types::string::RadonString;

    let input = RadonString::from("Hello world!").into();
    let expected = RadonString::from("Hello world!").into();
    let call = (RadonOpCodes::Identity, None);
    let output = operate(input, &call).unwrap();

    assert_eq!(output, expected);
}
