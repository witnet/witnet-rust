use std::fmt;

use num_enum::TryFromPrimitive;

use crate::{
    error::RadError,
    types::{array::RadonArray, RadonType, RadonTypes},
};
use witnet_data_structures::radon_report::ReportContext;

pub mod average;
pub mod deviation;
pub mod median;
pub mod mode;

#[derive(Debug, PartialEq, TryFromPrimitive)]
#[repr(u8)]
pub enum RadonReducers {
    // Implemented
    Mode = 0x02,
    AverageMean = 0x03,
    AverageMedian = 0x05,
    DeviationStandard = 0x07,

    // Not implemented
    Min = 0x00,
    Max = 0x01,
    AverageMeanWeighted = 0x04,
    AverageMedianWeighted = 0x06,
    DeviationAverageAbsolute = 0x08,
    DeviationMedianAbsolute = 0x09,
    DeviationMaximumAbsolute = 0x10,
}

impl fmt::Display for RadonReducers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RadonReducers::{:?}", self)
    }
}

pub fn reduce(
    input: &RadonArray,
    reducer_code: RadonReducers,
    context: &mut ReportContext<RadonTypes>,
) -> Result<RadonTypes, RadError> {
    let error = || {
        Err(RadError::UnsupportedReducer {
            array: input.clone(),
            reducer: reducer_code.to_string(),
        })
    };

    if input.is_homogeneous() || input.value().is_empty() {
        match reducer_code {
            RadonReducers::AverageMean => {
                average::mean(input, average::MeanReturnPolicy::RoundToInteger)
            }
            RadonReducers::Mode => mode::mode(input),
            RadonReducers::DeviationStandard => deviation::standard(input),
            RadonReducers::AverageMedian => match &context.active_wips {
                Some(active_wips) if active_wips.wip0017() => median::median(input),
                _ => error(),
            },
            _ => error(),
        }
    } else {
        Err(RadError::UnsupportedOpNonHomogeneous {
            operator: reducer_code.to_string(),
        })
    }
}
