use crate::error::RadError;
use crate::types::boolean::RadonBoolean;
use crate::types::float::RadonFloat;
use crate::types::RadonType;
use rmpv::Value;

pub fn multiply(input: &RadonFloat, args: &[Value]) -> Result<RadonFloat, RadError> {
    match args.first().map(Value::as_f64).unwrap_or(None) {
        Some(multiplier) => Ok(RadonFloat::from(input.value() * multiplier)),
        None => Err(RadError::WrongArguments {
            input_type: input.to_string(),
            operator: "FloatMultiply".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn greater_than(input: &RadonFloat, args: &[Value]) -> Result<RadonBoolean, RadError> {
    match args.first().map(Value::as_f64).unwrap_or(None) {
        Some(other) => Ok(RadonBoolean::from(input.value() > other)),
        None => Err(RadError::WrongArguments {
            input_type: input.to_string(),
            operator: "FloatGreaterThan".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn less_than(input: &RadonFloat, args: &[Value]) -> Result<RadonBoolean, RadError> {
    match args.first().map(Value::as_f64).unwrap_or(None) {
        Some(other) => Ok(RadonBoolean::from(input.value() < other)),
        None => Err(RadError::WrongArguments {
            input_type: input.to_string(),
            operator: "FloatLessThan".to_string(),
            args: args.to_vec(),
        }),
    }
}
