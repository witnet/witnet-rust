use crate::error::*;
use crate::types::{array::RadonArray, RadonTypes};

mod average;

use std::fmt;
use num_derive::FromPrimitive;

#[derive(Debug, FromPrimitive, PartialEq)]
pub enum RadonReducers {
    Mode = 0x10,
    AverageMean = 0x20,
    AverageMeanWeighted = 0x21,
    AverageMedian = 0x22,
    AverageMedianWeighted = 0x23,
    DeviationStandard = 0x30,
    DeviationAverageAbsolute = 0x31,
    DeviationMedianAbsolute = 0x32,
    DeviationMaximumAbsolute = 0x33,
}

impl fmt::Display for RadonReducers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RadonReducers::{:?}", self)
    }
}

pub fn reduce(input: &RadonArray, reducer_code: RadonReducers) -> RadResult<RadonTypes> {
    match reducer_code {
        RadonReducers::AverageMean => {
            average::mean(input)
        }
        _ => {
            Err(WitnetError::from(RadError::new(
                RadErrorKind::UnsupportedReducer,
                format!(
                    "Call to reducer {:} is not supported on the provided type of RadonArray",
                    reducer_code
                ),
            )))
        }
    }
}