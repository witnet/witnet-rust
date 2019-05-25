// FIXME: https://github.com/rust-num/num-derive/issues/20
#![allow(clippy::useless_attribute)]

use crate::error::RadError;
use crate::types::{array::RadonArray, RadonTypes};

mod average;

use num_derive::FromPrimitive;
use std::fmt;

#[derive(Debug, FromPrimitive, PartialEq)]
pub enum RadonReducers {
    Min = 0x00,
    Max = 0x01,
    Mode = 0x02,
    AverageMean = 0x03,
    AverageMeanWeighted = 0x04,
    AverageMedian = 0x05,
    AverageMedianWeighted = 0x06,
    DeviationStandard = 0x07,
    DeviationAverageAbsolute = 0x08,
    DeviationMedianAbsolute = 0x09,
    DeviationMaximumAbsolute = 0x10,
}

impl fmt::Display for RadonReducers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RadonReducers::{:?}", self)
    }
}

pub fn reduce(input: &RadonArray, reducer_code: RadonReducers) -> Result<RadonTypes, RadError> {
    let error = || {
        Err(RadError::UnsupportedReducer {
            inner_type: format!("{:?}", input.inner_type()),
            reducer: reducer_code.to_string(),
        })
    };

    if input.is_homogeneous() {
        match reducer_code {
            RadonReducers::AverageMean => average::mean(input),
            _ => error(),
        }
    } else {
        error()
    }
}
