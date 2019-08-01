use std::{convert::TryInto, fmt};

use rmpv::Value;
use serde::{Serialize, Serializer};

use crate::error::RadError;
use crate::operators::{bytes as bytes_operators, identity, Operable, RadonOpCodes};
use crate::script::RadonCall;
use crate::types::{RadonType, RadonTypes};

pub const RADON_BYTES_TYPE_NAME: &str = "RadonBytes";

#[derive(Clone, Debug, PartialEq)]
pub struct RadonBytes {
    value: Value,
}

impl Default for RadonBytes {
    fn default() -> Self {
        Self { value: Value::Nil }
    }
}

impl Serialize for RadonBytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.value.to_string())
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
    fn operate(self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        match call {
            // Identity
            (RadonOpCodes::Identity, None) => identity(RadonTypes::Bytes(self)),
            // To Float
            (RadonOpCodes::BytesToFloat, None) => bytes_operators::to_float(self)
                .map(RadonTypes::from)
                .map_err(Into::into),
            // To Array
            (RadonOpCodes::BytesToArray, None) => bytes_operators::to_array(self)
                .map(RadonTypes::from)
                .map_err(Into::into),
            // To Map
            (RadonOpCodes::BytesToMap, None) => bytes_operators::to_map(self)
                .map(RadonTypes::from)
                .map_err(Into::into),
            // Unsupported / unimplemented
            (op_code, args) => Err(RadError::UnsupportedOperator {
                input_type: RADON_BYTES_TYPE_NAME.to_string(),
                operator: op_code.to_string(),
                args: args.to_owned(),
            }),
        }
    }
}

impl fmt::Display for RadonBytes {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}({:?})", RADON_BYTES_TYPE_NAME, self.value)
    }
}

#[test]
fn test_operate_identity() {
    let value = rmpv::Value::from(0);
    let input = RadonBytes::from(value.clone());
    let expected = RadonTypes::Bytes(RadonBytes::from(value));

    let call = (RadonOpCodes::Identity, None);
    let output = input.operate(&call).unwrap();

    assert_eq!(output, expected);
}

#[test]
fn test_operate_unimplemented() {
    let input = RadonBytes::from(rmpv::Value::from(0));

    let call = (RadonOpCodes::Fail, None);
    let result = input.operate(&call);

    assert!(if let Err(_error) = result {
        true
    } else {
        false
    });
}
