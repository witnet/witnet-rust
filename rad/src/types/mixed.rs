use std::{convert::TryInto, fmt};

use serde::{Serialize, Serializer};
use serde_cbor::value::Value;

use crate::{
    error::RadError,
    operators::{identity, mixed as mixed_operators, Operable, RadonOpCodes},
    script::RadonCall,
    types::{RadonType, RadonTypes},
};
use std::convert::TryFrom;
use witnet_data_structures::radon_report::ReportContext;

pub const RADON_MIXED_TYPE_NAME: &str = "RadonMixed";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RadonMixed {
    value: Value,
}

impl Default for RadonMixed {
    fn default() -> Self {
        Self { value: Value::Null }
    }
}

impl Serialize for RadonMixed {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.value().serialize(serializer)
    }
}

impl RadonType<Value> for RadonMixed {
    fn value(&self) -> Value {
        self.value.clone()
    }

    fn radon_type_name() -> String {
        RADON_MIXED_TYPE_NAME.to_string()
    }
}

impl From<Value> for RadonMixed {
    fn from(value: Value) -> Self {
        RadonMixed { value }
    }
}

impl TryFrom<RadonTypes> for RadonMixed {
    type Error = RadError;

    fn try_from(item: RadonTypes) -> Result<Self, Self::Error> {
        if let RadonTypes::Mixed(rad_mixed) = item {
            Ok(rad_mixed)
        } else {
            let value = Value::try_from(item)?;
            Ok(RadonMixed { value })
        }
    }
}

impl TryInto<Value> for RadonMixed {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        Ok(self.value())
    }
}

impl Operable for RadonMixed {
    fn operate(&self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        match call {
            // Identity
            (RadonOpCodes::Identity, None) => identity(RadonTypes::from(self.clone())),
            // To Float
            (RadonOpCodes::MixedAsFloat, None) => {
                mixed_operators::to_float(self.clone()).map(RadonTypes::from)
            }
            // To Integer
            (RadonOpCodes::MixedAsInteger, None) => {
                mixed_operators::to_int(self.clone()).map(RadonTypes::from)
            }
            // To Array
            (RadonOpCodes::MixedAsArray, None) => {
                mixed_operators::to_array(self.clone()).map(RadonTypes::from)
            }
            // To Map
            (RadonOpCodes::MixedAsMap, None) => {
                mixed_operators::to_map(self.clone()).map(RadonTypes::from)
            }
            // To Boolean
            (RadonOpCodes::MixedAsBoolean, None) => {
                mixed_operators::to_bool(self.clone()).map(RadonTypes::from)
            }
            // To String
            (RadonOpCodes::MixedAsString, None) => {
                mixed_operators::to_string(self.clone()).map(RadonTypes::from)
            }
            // Unsupported / unimplemented
            (op_code, args) => Err(RadError::UnsupportedOperator {
                input_type: RADON_MIXED_TYPE_NAME.to_string(),
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

impl fmt::Display for RadonMixed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}({:?})", RADON_MIXED_TYPE_NAME, self.value)
    }
}

#[test]
fn test_operate_identity() {
    use std::convert::TryFrom;

    let value = Value::try_from(0x00).unwrap();
    let input = RadonMixed::from(value.clone());
    let expected = RadonTypes::Mixed(RadonMixed::from(value));

    let call = (RadonOpCodes::Identity, None);
    let output = input.operate(&call).unwrap();

    assert_eq!(output, expected);
}

#[test]
fn test_operate_unimplemented() {
    use std::convert::TryFrom;

    let input = RadonMixed::from(Value::try_from(0).unwrap());

    let call = (RadonOpCodes::Fail, None);
    let result = input.operate(&call);

    assert!(if let Err(_error) = result {
        true
    } else {
        false
    });
}
