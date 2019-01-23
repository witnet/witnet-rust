use crate::error::*;
use crate::reducers::{self, RadonReducers};
use crate::types::{array::RadonArray, RadonTypes};

use num_traits::FromPrimitive;
use rmpv::Value;

pub fn reduce(input: &RadonArray, args: &[Value]) -> RadResult<RadonTypes> {
    let none_error = || WitnetError::from(RadError::new(RadErrorKind::None, String::from("")));

    let reducer_integer = args
        .first()
        .ok_or_else(none_error)?
        .as_i64()
        .ok_or_else(none_error)?;
    let reducer_code = RadonReducers::from_i64(reducer_integer).ok_or_else(none_error)?;

    reducers::reduce(input, reducer_code)
}
