// FIXME: https://github.com/rust-num/num-derive/issues/20
#![allow(clippy::useless_attribute)]

use crate::error::RadError;
use crate::script::RadonCall;
use crate::types::RadonTypes;
use num_derive::FromPrimitive;
use std::fmt;

pub mod array;
pub mod boolean;
pub mod bytes;
pub mod float;
pub mod map;
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

    ///////////////////////////////////////////////////////////////////////
    // Boolean operator codes (start at 0x10)
    //    BooleanMatch = 0x10,
    BooleanNegate = 0x11,
    //    BooleanAsString = 0x12,

    // Integer operator codes (start at 0x20)
    //    IntegerAbsolute = 0x20,
    //    IntegerAsBytes = 0x21,
    //    IntegerAsFloat = 0x22,
    //    IntegerAsString = 0x23,
    //    IntegerGreaterThan = 0x24,
    //    IntegerLessThan = 0x25,
    //    IntegerMatch = 0x26,
    //    IntegerModulo = 0x27,
    //    IntegerMultiply = 0x28,
    //    IntegerNegate = 0x29,
    //    IntegerPower = 0x2A,
    //    IntegerReciprocal = 0x2B,
    //    IntegerSum = 0x2C,

    // Float operator codes (start at 0x30)
    //    FloatAbsolute = 0x30,
    //    FloatAsBytes = 0x31,
    //    FloatAsString = 0x32,
    //    FloatCeiling = 0x33,
    FloatGreaterThan = 0x34,
    //    FloatFloor = 0x35,
    FloatLessThan = 0x36,
    //    FloatModulo = 0x37,
    FloatMultiply = 0x38,
    //    FloatNegate = 0x39,
    //    FloatPower = 0x3A,
    //    FloatReciprocal = 0x3B,
    //    FloatRound = 0x3C,
    //    FloatSum = 0x3D,
    //    FloatTruncate = 0x3E,

    // String operator codes (start at 0x40)
    //    StringAsBytes = 0x40,
    StringAsFloat = 0x41,
    //    StringAsInteger = 0x42,
    //    StringLength = 0x43,
    //    StringMatch = 0x44,
    /// Parse Bytes from JSON string
    StringParseJSON = 0x45,
    //    StringParseXML = 0x46,
    //    StringAsBoolean = 0x47,
    //    StringToLowerCase = 0x48,
    //    StringToUpperCase = 0x49,

    // Array operator codes (start at 0x50)
    //    ArrayAsBytes = 0x50,
    //    ArrayCount = 0x51,
    //    ArrayEvery = 0x52,
    //    ArrayFilter = 0x53,
    //    ArrayFlatten = 0x54,
    ArrayGet = 0x55,
    ArrayMap = 0x56,
    ArrayReduce = 0x57,
    //    ArraySome = 0x58,
    //    ArraySort = 0x59,
    //    ArrayTake = 0x5A,

    // Map operator codes (start at 0x60)
    //    MapEntries = 0x60,
    MapGet = 0x61,
    //    MapKeys = 0x62,
    /// Flatten a map into an Array containing only the values but not the keys
    MapValues = 0x63,
    // Bytes operator codes (start at 0x70)
    BytesAsArray = 0x70,
    //    BytesAsBoolean = 0x71,
    BytesAsFloat = 0x72,
    //    BytesAsInteger = 0x73,
    BytesAsMap = 0x74,
    //    BytesAsString = 0x75,
    //    BytesHash = 0x76,

    // Result operator codes (start at 0x80)
    //    ResultGet = 0x80,
    //    ResultGetOr = 0x81,
    //    ResultIsOk = 0x82,
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
        RadonTypes::Bytes(radon_bytes) => radon_bytes.operate(call),
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
