use std::{convert::TryInto, fmt};

use serde::{Serialize, Serializer};
use serde_cbor::value::Value;

use crate::error::RadError;
use crate::operators::{bytes as bytes_operators, identity, Operable, RadonOpCodes};
use crate::script::RadonCall;
use crate::types::{RadonType, RadonTypes};
use witnet_data_structures::radon_report::ReportContext;

pub const RADON_BYTES_TYPE_NAME: &str = "RadonBytes";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RadonBytes {
    value: Value,
}

impl Default for RadonBytes {
    fn default() -> Self {
        Self { value: Value::Null }
    }
}

impl Serialize for RadonBytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.value().serialize(serializer)
    }
}

impl RadonType<Value> for RadonBytes {
    fn value(&self) -> Value {
        self.value.clone()
    }

    fn radon_type_name() -> String {
        RADON_BYTES_TYPE_NAME.to_string()
    }
}

impl From<Value> for RadonBytes {
    fn from(value: Value) -> Self {
        RadonBytes { value }
    }
}

impl TryInto<Value> for RadonBytes {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        Ok(self.value())
    }
}

impl Operable for RadonBytes {
    fn operate(&self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        match call {
            // Identity
            (RadonOpCodes::Identity, None) => identity(RadonTypes::from(self.clone())),
            // To Float
            (RadonOpCodes::BytesAsFloat, None) => {
                bytes_operators::to_float(self.clone()).map(RadonTypes::from)
            }
            // To Integer
            (RadonOpCodes::BytesAsInteger, None) => {
                bytes_operators::to_int(self.clone()).map(RadonTypes::from)
            }
            // To Array
            (RadonOpCodes::BytesAsArray, None) => {
                bytes_operators::to_array(self.clone()).map(RadonTypes::from)
            }
            // To Map
            (RadonOpCodes::BytesAsMap, None) => {
                bytes_operators::to_map(self.clone()).map(RadonTypes::from)
            }
            // To Boolean
            (RadonOpCodes::BytesAsBoolean, None) => {
                bytes_operators::to_bool(self.clone()).map(RadonTypes::from)
            }
            // To String
            (RadonOpCodes::BytesAsString, None) => {
                bytes_operators::to_string(self.clone()).map(RadonTypes::from)
            }
            // Unsupported / unimplemented
            (op_code, args) => Err(RadError::UnsupportedOperator {
                input_type: RADON_BYTES_TYPE_NAME.to_string(),
                operator: op_code.to_string(),
                args: args.to_owned(),
            }),
        }
    }

    fn operate_in_context(
        &self,
        call: &RadonCall,
        _context: &mut ReportContext,
    ) -> Result<RadonTypes, RadError> {
        self.operate(call)
    }
}

impl fmt::Display for RadonBytes {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}({:?})", RADON_BYTES_TYPE_NAME, self.value)
    }
}

#[test]
fn test_operate_identity() {
    use std::convert::TryFrom;

    let value = Value::try_from(0x00).unwrap();
    let input = RadonBytes::from(value.clone());
    let expected = RadonTypes::Bytes(RadonBytes::from(value));

    let call = (RadonOpCodes::Identity, None);
    let output = input.operate(&call).unwrap();

    assert_eq!(output, expected);
}

#[test]
fn test_operate_unimplemented() {
    use std::convert::TryFrom;

    let input = RadonBytes::from(Value::try_from(0).unwrap());

    let call = (RadonOpCodes::Fail, None);
    let result = input.operate(&call);

    assert!(if let Err(_error) = result {
        true
    } else {
        false
    });
}
