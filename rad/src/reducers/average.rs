use crate::error::RadResult;
use crate::types::{array::RadonArray, float::RadonFloat, RadonType, RadonTypes};

use std::ops::Div;

pub fn mean(input: &RadonArray) -> RadResult<RadonTypes> {
    let value = input.value();

    // Sum all numeric values
    let (sum, count) = value
        .iter()
        .fold((0f64, 0f64), |(sum, count), item| match item {
            RadonTypes::Float(f64_value) => (sum + f64_value.value(), count + 1f64),
            // TODO: implement RadonInteger branch here
            // RadonTypes::Integer(i64_value) => (sum + acc + i64_value.value() as f64, count + 1f64),
            // Skip any non-numeric RadonType
            _ => (sum, count),
        });

    // Divide sum by the count of numeric values that were summed
    let mean_value: f64 = sum.div(count);

    Ok(RadonTypes::from(RadonFloat::from(mean_value)))
}
