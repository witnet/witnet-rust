use std::fmt;

use num_enum::TryFromPrimitive;

use crate::{
    error::RadError,
    types::{array::RadonArray, RadonType, RadonTypes},
};

pub mod average;
pub mod deviation;
pub mod mode;

#[derive(Debug, PartialEq, TryFromPrimitive)]
#[repr(u8)]
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
            array: input.clone(),
            reducer: reducer_code.to_string(),
        })
    };

    if input.is_homogeneous() || input.value().is_empty() {
        match reducer_code {
            RadonReducers::AverageMean => average::mean(input),
            RadonReducers::Mode => mode::mode(input),
            RadonReducers::DeviationStandard => deviation::standard(input),
            _ => error(),
        }
    } else {
        Err(RadError::UnsupportedOpNonHomogeneous {
            operator: reducer_code.to_string(),
        })
    }
}
