use std::convert::{TryFrom, TryInto};
use std::fmt;

use serde_cbor::value::{from_value, Value};

use crate::{
    error::RadError,
    operators::{boolean as boolean_operators, identity, Operable, RadonOpCodes},
    script::RadonCall,
    types::{RadonType, RadonTypes},
};
use witnet_data_structures::radon_report::ReportContext;

const RADON_BOOLEAN_TYPE_NAME: &str = "RadonBoolean";

#[derive(Clone, Debug, PartialEq, Eq, Default)]
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

    #[inline]
    fn radon_type_name() -> &'static str {
        RADON_BOOLEAN_TYPE_NAME
    }
}

impl TryFrom<Value> for RadonBoolean {
    type Error = RadError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        from_value::<bool>(value)
            .map_err(|_| RadError::Decode {
                from: "cbor::value::Value",
                to: RadonBoolean::radon_type_name(),
            })
            .map(Self::from)
    }
}

impl TryFrom<RadonTypes> for RadonBoolean {
    type Error = RadError;

    fn try_from(item: RadonTypes) -> Result<Self, Self::Error> {
        if let RadonTypes::Boolean(rad_bool) = item {
            Ok(rad_bool)
        } else {
            let value = Value::try_from(item)?;
            value.try_into()
        }
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
    fn operate(&self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        match call {
            (RadonOpCodes::Identity, None) => identity(RadonTypes::from(self.clone())),
            (RadonOpCodes::BooleanNegate, None) => Ok(boolean_operators::negate(self).into()),
            (RadonOpCodes::BooleanAsString, None) => boolean_operators::to_string(self.clone())
                .map(RadonTypes::from)
                .map_err(Into::into),
            (op_code, args) => Err(RadError::UnsupportedOperator {
                input_type: RADON_BOOLEAN_TYPE_NAME.to_string(),
                operator: op_code.to_string(),
                args: args.to_owned(),
            }),
        }
    }

    fn operate_in_context(
        &self,
        call: &RadonCall,
        _context: &mut ReportContext<RadonTypes>,
    ) -> Result<RadonTypes, RadError> {
        self.operate(call)
    }
}
