// FIXME: https://github.com/rust-num/num-derive/issues/20
#![allow(clippy::useless_attribute)]

use crate::error::*;
use crate::types::{array::RadonArray, RadonTypes};

mod average;

use num_derive::FromPrimitive;
use std::fmt;

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
    if input.is_homogeneous() {
        match reducer_code {
            RadonReducers::AverageMean => average::mean(input),
            _ => Err(WitnetError::from(RadError::new(
                RadErrorKind::UnsupportedReducer,
                format!("Reducer {:} is not yet implemented", reducer_code),
            ))),
        }
    } else {
        Err(WitnetError::from(RadError::new(
            RadErrorKind::UnsupportedReducer,
            String::from("Heterogeneous arrays (RadonArray<Mixed>) can not be reduced"),
        )))
    }
}
