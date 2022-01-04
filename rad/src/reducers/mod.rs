use std::fmt;

use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::{
    error::RadError,
    types::{array::RadonArray, RadonType, RadonTypes},
};
use witnet_data_structures::radon_report::ReportContext;

pub mod average;
pub mod deviation;
pub mod hash_concatenate;
pub mod median;
pub mod mode;

#[derive(Debug, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum RadonReducers {
    // Implemented
    Mode = 0x02,
    AverageMean = 0x03,
    AverageMedian = 0x05,
    DeviationStandard = 0x07,
    HashConcatenate = 0x0b,

    // Not implemented
    Min = 0x00,
    Max = 0x01,
    AverageMeanWeighted = 0x04,
    AverageMedianWeighted = 0x06,
    DeviationAverageAbsolute = 0x08,
    DeviationMedianAbsolute = 0x09,
    DeviationMaximumAbsolute = 0x0a,
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
            RadonReducers::HashConcatenate => match &context.active_wips {
                Some(active_wips) if active_wips.wip0019() => {
                    hash_concatenate::hash_concatenate(input)
                }
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

#[cfg(test)]
mod tests {
    use crate::{
        current_active_wips,
        error::RadError,
        reducers::{reduce, RadonReducers},
        types::{array::RadonArray, float::RadonFloat, RadonTypes},
    };
    use witnet_data_structures::radon_report::ReportContext;

    #[test]
    fn test_reduce_average_mean_float() {
        let input = &RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]);
        let expected = RadonTypes::from(RadonFloat::from(1.5f64));

        let output = reduce(
            input,
            RadonReducers::AverageMean,
            &mut ReportContext::default(),
        )
        .unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_reduce_deviation_standard_float() {
        let input = &RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]);
        let expected = RadonTypes::from(RadonFloat::from(0.5));

        let output = reduce(
            input,
            RadonReducers::DeviationStandard,
            &mut ReportContext::default(),
        )
        .unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_reduce_average_median() {
        let mut context = ReportContext {
            active_wips: Some(current_active_wips()),
            ..Default::default()
        };
        let input = &RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
            RadonFloat::from(2f64).into(),
        ]);

        let expected = RadonTypes::from(RadonFloat::from(2f64));
        let output = reduce(input, RadonReducers::AverageMedian, &mut context).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_reduce_average_median_tapi_activation() {
        let mut active_wips = current_active_wips();
        let mut context = ReportContext {
            active_wips: Some(active_wips.clone()),
            ..Default::default()
        };
        let input = &RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
            RadonFloat::from(2f64).into(),
        ]);

        let expected = RadonTypes::from(RadonFloat::from(2f64));
        let output = reduce(input, RadonReducers::AverageMedian, &mut context).unwrap();

        assert_eq!(output, expected);

        // Deactivate WIP-0017
        active_wips
            .active_wips
            .remove(&"WIP0017-0018-0019".to_string());
        context.active_wips = Some(active_wips);

        let expected_err = RadError::UnsupportedReducer {
            array: input.clone(),
            reducer: "RadonReducers::AverageMedian".to_string(),
        };
        let output = reduce(input, RadonReducers::AverageMedian, &mut context).unwrap_err();

        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_reduce_mode_float() {
        let input = &RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
            RadonFloat::from(2f64).into(),
        ]);
        let expected = RadonTypes::from(RadonFloat::from(2f64));
        let output = reduce(input, RadonReducers::Mode, &mut ReportContext::default()).unwrap();
        assert_eq!(output, expected);
    }
}
