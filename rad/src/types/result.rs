use std::convert::{TryFrom, TryInto};
use std::fmt;

use serde::Serialize;
use serde_cbor::value::Value;

use crate::operators::Operable;
use crate::rad_error::RadError;
use crate::report::ReportContext;
use crate::script::RadonCall;
use crate::types::{RadonType, RadonTypes};

pub const RADON_RESULT_TYPE_NAME: &str = "RadonResult";

type LocalResult = Result<Box<RadonTypes>, u8>;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RadonResult {
    value: LocalResult,
}

impl From<LocalResult> for RadonResult {
    fn from(input: LocalResult) -> Self {
        Self { value: input }
    }
}

impl RadonType<LocalResult> for RadonResult {
    fn value(&self) -> LocalResult {
        self.value.to_owned()
    }

    fn radon_type_name() -> String {
        RADON_RESULT_TYPE_NAME.to_string()
    }
}

impl TryFrom<Value> for RadonResult {
    type Error = RadError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        Ok(Self::from(Ok(RadonTypes::try_from(value).map(Box::new)?)))
    }
}

impl TryInto<Value> for RadonResult {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        // Will fail if the result is errored, as serde_cbor::value::Value does not support tags.
        let value = *self.value.map_err(|_error_code| RadError::Encode {
            from: String::from(RADON_RESULT_TYPE_NAME),
            to: String::from("serde_cbor::value::Value"),
        })?;
        value.try_into()
    }
}

impl fmt::Display for RadonResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let inner = match &self.value {
            Ok(inner) => format!("Ok({})", inner),
            Err(inner) => format!("Err({})", inner),
        };
        write!(f, "{}::{:?}", RADON_RESULT_TYPE_NAME, inner)
    }
}

impl Operable for RadonResult {
    fn operate(&self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        match call {
            (op_code, args) => Err(RadError::UnsupportedOperator {
                input_type: RADON_RESULT_TYPE_NAME.to_string(),
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
