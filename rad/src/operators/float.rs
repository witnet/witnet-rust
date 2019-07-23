use std::borrow::ToOwned;

use serde_cbor::value::{from_value, Value};

use crate::error::RadError;
use crate::types::boolean::RadonBoolean;
use crate::types::float::RadonFloat;
use crate::types::RadonType;

pub fn multiply(input: &RadonFloat, args: &[Value]) -> Result<RadonFloat, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonFloat::radon_type_name(),
        operator: "Multiply".to_string(),
        args: args.to_vec(),
    };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let multiplier = from_value::<f64>(arg).map_err(|_| wrong_args())?;
    Ok(RadonFloat::from(input.value() * multiplier))
}

pub fn greater_than(input: &RadonFloat, args: &[Value]) -> Result<RadonBoolean, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonFloat::radon_type_name(),
        operator: "GreaterThan".to_string(),
        args: args.to_vec(),
    };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let other = from_value::<f64>(arg).map_err(|_| wrong_args())?;
    Ok(RadonBoolean::from(input.value() > other))
}

pub fn less_than(input: &RadonFloat, args: &[Value]) -> Result<RadonBoolean, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonFloat::radon_type_name(),
        operator: "LessThan".to_string(),
        args: args.to_vec(),
    };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let other = from_value::<f64>(arg).map_err(|_| wrong_args())?;
    Ok(RadonBoolean::from(input.value() < other))
}
