use std::fmt;

use num_enum::TryFromPrimitive;
use serde::Serialize;

use witnet_data_structures::radon_report::ReportContext;

use crate::{error::RadError, script::RadonCall, types::RadonTypes};

pub mod array;
pub mod boolean;
pub mod bytes;
pub mod float;
pub mod integer;
pub mod map;
pub mod string;

/// List of RADON operators.
/// **WARNING: these codes are consensus-critical.** They can be renamed but they cannot be
/// re-assigned without causing a non-backwards-compatible protocol upgrade.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, TryFromPrimitive)]
#[repr(u8)]
pub enum RadonOpCodes {
    /// Only for the sake of allowing catch-alls when matching
    Fail = 0xFF,
    ///////////////////////////////////////////////////////////////////////
    // Multi-type operator codes start at 0x00
    Identity = 0x00,
    ///////////////////////////////////////////////////////////////////////
    // Array operator codes (start at 0x10)
    ArrayCount = 0x10,
    ArrayFilter = 0x11,
    //    ArrayFlatten = 0x12,
    ArrayGetArray = 0x13,
    ArrayGetBoolean = 0x14,
    ArrayGetBytes = 0x15,
    ArrayGetFloat = 0x16,
    ArrayGetInteger = 0x17,
    ArrayGetMap = 0x18,
    ArrayGetString = 0x19,
    ArrayMap = 0x1A,
    ArrayReduce = 0x1B,
    //    ArraySome = 0x1C,
    ArraySort = 0x1D,
    //    ArrayTake = 0x1E,
    ///////////////////////////////////////////////////////////////////////
    // Boolean operator codes (start at 0x20)
    BooleanAsString = 0x20,
    //    BooleanMatch = 0x21,
    BooleanNegate = 0x22,
    ///////////////////////////////////////////////////////////////////////
    // Bytes operator codes (start at 0x30)
    BytesAsString = 0x30,
    BytesHash = 0x31,
    ///////////////////////////////////////////////////////////////////////
    // Integer operator codes (start at 0x40)
    IntegerAbsolute = 0x40,
    IntegerAsFloat = 0x41,
    IntegerAsString = 0x42,
    IntegerGreaterThan = 0x43,
    IntegerLessThan = 0x44,
    //    IntegerMatch = 0x45,
    IntegerModulo = 0x46,
    IntegerMultiply = 0x47,
    IntegerNegate = 0x48,
    IntegerPower = 0x49,
    //    IntegerReciprocal = 0x4A,
    //    IntegerSum = 0x4B,
    ///////////////////////////////////////////////////////////////////////
    // Float operator codes (start at 0x50)
    FloatAbsolute = 0x50,
    FloatAsString = 0x51,
    FloatCeiling = 0x52,
    FloatGreaterThan = 0x53,
    FloatFloor = 0x54,
    FloatLessThan = 0x55,
    FloatModulo = 0x56,
    FloatMultiply = 0x57,
    FloatNegate = 0x58,
    FloatPower = 0x59,
    //    FloatReciprocal = 0x5A,
    FloatRound = 0x5B,
    //    FloatSum = 0x5C,
    FloatTruncate = 0x5D,
    ///////////////////////////////////////////////////////////////////////
    // Map operator codes (start at 0x60)
    //    MapEntries = 0x60,
    MapGetArray = 0x61,
    MapGetBoolean = 0x62,
    MapGetBytes = 0x63,
    MapGetFloat = 0x64,
    MapGetInteger = 0x65,
    MapGetMap = 0x66,
    MapGetString = 0x67,
    MapKeys = 0x68,
    MapValues = 0x69,
    ///////////////////////////////////////////////////////////////////////
    // String operator codes (start at 0x70)
    StringAsBoolean = 0x70,
    //    StringAsBytes = 0x71,
    StringAsFloat = 0x72,
    StringAsInteger = 0x73,
    StringLength = 0x74,
    StringMatch = 0x75,
    StringParseJSONArray = 0x76,
    StringParseJSONMap = 0x77,
    StringParseXMLMap = 0x78,
    StringToLowerCase = 0x79,
    StringToUpperCase = 0x7A,
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
        context: &mut ReportContext<RadonTypes>,
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
    context: &mut ReportContext<RadonTypes>,
) -> Result<RadonTypes, RadError> {
    input.as_operable().operate_in_context(call, context)
}

pub fn identity(input: RadonTypes) -> Result<RadonTypes, RadError> {
    Ok(input)
}

#[cfg(test)]
mod tests {
    use crate::types::string::RadonString;

    use super::*;

    #[test]
    pub fn test_identity() {
        let input = RadonString::from("Hello world!").into();
        let expected = RadonString::from("Hello world!").into();
        let output = identity(input).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    pub fn test_operate() {
        let input = RadonString::from("Hello world!").into();
        let expected = RadonString::from("Hello world!").into();
        let call = (RadonOpCodes::Identity, None);
        let output = operate(input, &call).unwrap();

        assert_eq!(output, expected);
    }
}
