use std::convert::{TryFrom, TryInto};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_cbor::value::{from_value, Value};

use crate::operators::boolean as boolean_operators;
use crate::operators::{Operable, RadonOpCodes};
use crate::rad_error::RadError;
use crate::script::RadonCall;
use crate::types::{RadonType, RadonTypes};

pub const RADON_BOOLEAN_TYPE_NAME: &str = "RadonBoolean";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct RadonBoolean {
    value: bool,
}

impl From<bool> for RadonBoolean {
    fn from(input: bool) -> Self {
        Self { value: input }
    }
}

impl RadonType<bool> for RadonBoolean {
    fn value(&self) -> bool {
        self.value
    }

    fn radon_type_name() -> String {
        RADON_BOOLEAN_TYPE_NAME.to_string()
    }
}

impl TryFrom<Value> for RadonBoolean {
    type Error = RadError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        from_value::<bool>(value)
            .map_err(|_| RadError::Decode {
                from: "cbor::value::Value".to_string(),
                to: RADON_BOOLEAN_TYPE_NAME.to_string(),
            })
            .map(Self::from)
    }
}

impl TryInto<Value> for RadonBoolean {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        Ok(Value::from(self.value))
    }
}

impl fmt::Display for RadonBoolean {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}({:?})", RADON_BOOLEAN_TYPE_NAME, self.value)
    }
}

impl Operable for RadonBoolean {
    fn operate(self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        match call {
            (RadonOpCodes::BooleanNegate, None) => Ok(boolean_operators::negate(&self).into()),
            (RadonOpCodes::BooleanAsString, None) => boolean_operators::to_string(self)
                .map(RadonTypes::from)
                .map_err(Into::into),
            (op_code, args) => Err(RadError::UnsupportedOperator {
                input_type: RADON_BOOLEAN_TYPE_NAME.to_string(),
                operator: op_code.to_string(),
                args: args.to_owned(),
            }),
        }
    }
}
