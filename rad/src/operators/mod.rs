// FIXME: https://github.com/rust-num/num-derive/issues/20
#![allow(clippy::useless_attribute)]

use crate::error::RadError;
use crate::script::RadonCall;
use crate::types::RadonTypes;

use num_derive::FromPrimitive;
use std::fmt;

pub mod array;
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
    /// Array::get, Map::get, Result::get
    Get = 0x01,
    // Boolean operator codes start at 0x10
    // Integer operator codes start at 0x20
    // Float operator codes start at 0x30
    // Null operator codes start at 0x40
    // String operator codes start at 0x50
    /// Compute the hash of a string
    Hash = 0x50,
    /// Parse Mixed from JSON string
    ParseJson = 0x53,
    // Array operator codes start at 0x60
    Reduce = 0x66,
    // Map operator codes start at 0x70
    // Mixed operator codes start at 0x80
    ToFloat = 0x82,
    ToMap = 0x84,
    // Result operator codes start at 0x90
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
        RadonTypes::Float(radon_float) => radon_float.operate(call),
        RadonTypes::Map(radon_map) => radon_map.operate(call),
        RadonTypes::String(radon_string) => radon_string.operate(call),
        RadonTypes::Mixed(radon_mixed) => radon_mixed.operate(call),
    }
}

pub fn identity<'a>(input: RadonTypes) -> Result<RadonTypes, RadError> {
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
