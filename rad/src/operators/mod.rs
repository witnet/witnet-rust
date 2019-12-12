use std::fmt;

use num_enum::TryFromPrimitive;

use witnet_data_structures::radon_report::ReportContext;

use crate::{error::RadError, script::RadonCall, types::RadonTypes};

pub mod array;
pub mod boolean;
pub mod bytes;
pub mod float;
pub mod integer;
pub mod map;
pub mod mixed;
pub mod string;

/// List of RADON operators.
/// **WARNING: these codes are consensus-critical.** They can be renamed but they cannot be
/// re-assigned without causing a non-backwards-compatible protocol upgrade.
#[derive(Copy, Clone, Debug, PartialEq, TryFromPrimitive)]
#[repr(u8)]
pub enum RadonOpCodes {
    /// Only for the sake of allowing catch-alls when matching
    Fail = 0xFF,
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
    BooleanAsString = 0x12,

    // Integer operator codes (start at 0x20)
    IntegerAbsolute = 0x20,
    IntegerAsMixed = 0x21,
    IntegerAsFloat = 0x22,
    IntegerAsString = 0x23,
    IntegerGreaterThan = 0x24,
    IntegerLessThan = 0x25,
    //    IntegerMatch = 0x26,
    IntegerModulo = 0x27,
    IntegerMultiply = 0x28,
    IntegerNegate = 0x29,
    IntegerPower = 0x2A,
    //    IntegerReciprocal = 0x2B,
    //    IntegerSum = 0x2C,

    // Float operator codes (start at 0x30)
    FloatAbsolute = 0x30,
    FloatAsMixed = 0x31,
    FloatAsString = 0x32,
    FloatCeiling = 0x33,
    FloatGreaterThan = 0x34,
    FloatFloor = 0x35,
    FloatLessThan = 0x36,
    FloatModulo = 0x37,
    FloatMultiply = 0x38,
    FloatNegate = 0x39,
    FloatPower = 0x3A,
    //    FloatReciprocal = 0x3B,
    FloatRound = 0x3C,
    //    FloatSum = 0x3D,
    FloatTruncate = 0x3E,

    // String operator codes (start at 0x40)
    StringAsMixed = 0x40,
    StringAsFloat = 0x41,
    StringAsInteger = 0x42,
    StringLength = 0x43,
    StringMatch = 0x44,
    /// Parse Bytes from JSON string
    StringParseJSON = 0x45,
    //    StringParseXML = 0x46,
    StringAsBoolean = 0x47,
    StringToLowerCase = 0x48,
    StringToUpperCase = 0x49,

    // Array operator codes (start at 0x50)
    //    ArrayAsMixed = 0x50,
    ArrayCount = 0x51,
    //    ArrayEvery = 0x52,
    ArrayFilter = 0x53,
    //    ArrayFlatten = 0x54,
    ArrayGet = 0x55,
    ArrayMap = 0x56,
    ArrayReduce = 0x57,
    //    ArraySome = 0x58,
    ArraySort = 0x59,
    //    ArrayTake = 0x5A,

    // Map operator codes (start at 0x60)
    //    MapEntries = 0x60,
    MapGet = 0x61,
    MapKeys = 0x62,
    /// Flatten a map into an Array containing only the values but not the keys
    MapValues = 0x63,
    // Mixed operator codes (start at 0x70)
    MixedAsArray = 0x70,
    MixedAsBoolean = 0x71,
    MixedAsFloat = 0x72,
    MixedAsInteger = 0x73,
    MixedAsMap = 0x74,
    MixedAsString = 0x75,
    //    MixedHash = 0x76,

    // Result operator codes (start at 0x80)
    //    ResultGet = 0x80,
    //    ResultGetOr = 0x81,
    //    ResultIsOk = 0x82,

    // Bytes operator codes (start at 0x90)
    BytesAsString = 0x90,
    BytesHash = 0x91,
}

impl fmt::Display for RadonOpCodes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub trait Operable {
    fn operate(&self, call: &RadonCall) -> Result<RadonTypes, RadError>;

    fn operate_in_context(
        &self,
        call: &RadonCall,
        context: &mut ReportContext,
    ) -> Result<RadonTypes, RadError>;
}

pub fn operate(input: RadonTypes, call: &RadonCall) -> Result<RadonTypes, RadError> {
    input.as_operable().operate(call)
}

/// This is bound to be a replacement for the original `operate` method.
/// The main difference with the former is that it passes mutable references of the context down to
/// operators for them to put there whatever metadata they need to.
pub fn operate_in_context(
    input: RadonTypes,
    call: &RadonCall,
    context: &mut ReportContext,
) -> Result<RadonTypes, RadError> {
    input.as_operable().operate_in_context(call, context)
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
