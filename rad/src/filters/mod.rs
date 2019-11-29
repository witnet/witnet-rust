use std::fmt;

use num_enum::TryFromPrimitive;
use serde_cbor::Value;

use crate::error::RadError;
use crate::types::{array::RadonArray, RadonType, RadonTypes};
use witnet_data_structures::radon_report::ReportContext;

pub mod deviation;

#[derive(Debug, PartialEq, TryFromPrimitive)]
#[repr(u8)]
pub enum RadonFilters {
    GreaterThan = 0x00,
    LessThan = 0x01,
    Equals = 0x02,
    DeviationAbsolute = 0x03,
    DeviationRelative = 0x04,
    DeviationStandard = 0x05,
    Top = 0x06,
    Bottom = 0x07,
    LessOrEqualThan = 0x80,
    GreaterOrEqualThan = 0x81,
    NotEquals = 0x82,
    NotDeviationAbsolute = 0x83,
    NotDeviationRelative = 0x84,
    NotDeviationStandard = 0x85,
    NotTop = 0x86,
    NotBottom = 0x87,
}

impl fmt::Display for RadonFilters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RadonFilters::{:?}", self)
    }
}

pub fn filter(
    input: &RadonArray,
    filter_code: RadonFilters,
    extra_args: &[Value],
    context: &mut ReportContext,
) -> Result<RadonTypes, RadError> {
    let error = || {
        Err(RadError::UnsupportedFilter {
            inner_type: format!("{:?}", input.inner_type()),
            filter: filter_code.to_string(),
        })
    };

    if input.is_homogeneous() || input.value().is_empty() {
        match filter_code {
            RadonFilters::DeviationStandard => {
                deviation::standard_filter(input, extra_args, context)
            }
            _ => error(),
        }
    } else {
        Err(RadError::UnsupportedOpNonHomogeneous {
            operator: filter_code.to_string(),
        })
    }
}
