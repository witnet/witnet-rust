use crate::error::RadError;
use crate::operators::boolean as boolean_operators;
use crate::operators::{Operable, RadonOpCodes};
use crate::script::RadonCall;
use crate::types::{RadonType, RadonTypes};
use rmpv::Value;
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use std::fmt;

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
        value
            .as_bool()
            .ok_or_else(|| RadError::Decode {
                from: "rmpv::Value".to_string(),
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
            (op_code, args) => Err(RadError::UnsupportedOperator {
                input_type: RADON_BOOLEAN_TYPE_NAME.to_string(),
                operator: op_code.to_string(),
                args: args.to_owned(),
            }),
        }
    }
}
