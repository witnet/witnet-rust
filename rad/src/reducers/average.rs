use crate::error::RadResult;
use crate::types::{array::RadonArray, RadonType, RadonTypes};

pub fn mean(input: &RadonArray) -> RadResult<RadonTypes> {
    input.value().iter().try_fold()
}