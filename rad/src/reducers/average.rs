use std::ops::Div;

use crate::{
    error::RadError,
    operators::{array as array_operators, float as float_operators},
    reducers::RadonReducers,
    types::{array::RadonArray, float::RadonFloat, integer::RadonInteger, RadonType, RadonTypes},
};

/// Different available policies regarding what to do with the resulting Float after applying the
/// average mean.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MeanReturnPolicy {
    /// Pure `Fn(Array<T>) -> Float` behavior.
    ReturnFloat,
    /// Enforces `Fn(Array<T>) -> T` behavior (if needed).
    RoundToInteger,
}

/// Computes the average mean of the values found in a `RadonArray`.
///
/// Please note that `input` is assumed to be homogeneus.
///
/// # Return policy
///
/// Depending on the `return_policy` parameter, it will enforce the reducer contract, i.e.
/// `Fn(Array<T>) -> T`. In other words, if the input was `Integer`, and the policy is
/// `RoundToInteger` it will round the final `Float` so the input and output types do match.
///
/// The standard behavior for the `AverageMean` reducer is with `RoundToInteger`. However, the
/// `DeviationStandard` reducer should call it with `ReturnFloat`.
///
/// # Examples
///
/// ```rust
/// use witnet_rad::{
///     reducers::average::{
///         mean, MeanReturnPolicy
///     },
///     types::{
///         array::RadonArray,
///         float::RadonFloat,
///         integer::RadonInteger,
///         RadonTypes,
///     },
/// };
///
/// let integer_values = RadonArray::from(vec![
///     RadonTypes::Integer(RadonInteger::from(1)),
///     RadonTypes::Integer(RadonInteger::from(2))
/// ]);
///
/// let integer_mean = RadonTypes::Integer(RadonInteger::from(2));
/// let float_mean = RadonTypes::Float(RadonFloat::from(1.5));
///
/// assert_eq!(mean(&integer_values, MeanReturnPolicy::RoundToInteger), Ok(integer_mean));
/// assert_eq!(mean(&integer_values, MeanReturnPolicy::ReturnFloat), Ok(float_mean));
/// ```
// FIXME: Allow for now, since there is no safe cast function from a usize to float yet
#[allow(clippy::cast_precision_loss)]
pub fn mean(input: &RadonArray, return_policy: MeanReturnPolicy) -> Result<RadonTypes, RadError> {
    let value = input.value();
    let value_len = value.len();

    match value.first() {
        None => Ok(RadonTypes::from(RadonFloat::from(std::f64::NAN))),
        Some(RadonTypes::Float(_)) => {
            let sum = value.iter().try_fold(0f64, |sum, item| match item {
                RadonTypes::Float(f64_value) => Ok(sum + f64_value.value()),
                _ => Err(RadError::MismatchingTypes {
                    method: RadonReducers::AverageMean.to_string(),
                    expected: RadonFloat::radon_type_name(),
                    found: item.clone().radon_type_name(),
                }),
            });
            let sum = sum?;

            // Divide sum by the count of numeric values that were summed
            let mean_value: f64 = sum.div(value_len as f64);

            Ok(RadonTypes::from(RadonFloat::from(mean_value)))
        }
        Some(RadonTypes::Integer(_)) => {
            let sum = value.iter().try_fold(0f64, |sum, item| match item {
                RadonTypes::Integer(i128_value) => Ok(sum + i128_value.value() as f64),
                _ => Err(RadError::MismatchingTypes {
                    method: RadonReducers::AverageMean.to_string(),
                    expected: RadonInteger::radon_type_name(),
                    found: item.clone().radon_type_name(),
                }),
            });
            let sum = sum?;

            // Divide sum by the count of numeric values that were summed
            let float_mean = RadonFloat::from(sum.div(value_len as f64));

            // In `RoundToInteger` mode, round the float so as to satisfy the reducers' inherent
            // contract.
            let mean = if return_policy == MeanReturnPolicy::RoundToInteger {
                RadonTypes::from(float_operators::round(&float_mean))
            } else {
                RadonTypes::from(float_mean)
            };

            Ok(mean)
        }
        Some(RadonTypes::Array(_)) => {
            let v = array_operators::transpose(input)?;

            let mut mean_v = vec![];
            for v2mean in v.value() {
                if let RadonTypes::Array(v2mean) = v2mean {
                    mean_v.push(mean(&v2mean, return_policy)?);
                } else {
                    unreachable!()
                }
            }

            Ok(RadonTypes::from(RadonArray::from(mean_v)))
        }
        Some(_rad_types) => Err(RadError::UnsupportedReducer {
            array: input.clone(),
            reducer: RadonReducers::AverageMean.to_string(),
        }),
    }
}

pub fn median(_input: &RadonArray) -> Result<RadonTypes, RadError> {
    // TODO: implement
    Ok(RadonTypes::Array(RadonArray::from(vec![])))
}

#[cfg(test)]
mod tests {
    use serde_cbor::Value;

    use crate::{
        operators::array::reduce,
        types::{float::RadonFloat, integer::RadonInteger, string::RadonString},
    };

    use super::*;
    use witnet_data_structures::radon_report::ReportContext;

    #[test]
    fn test_reduce_average_mean_float() {
        let input = &RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]);
        let args = &[Value::Integer(RadonReducers::AverageMean as i128)];
        let expected = RadonTypes::from(RadonFloat::from(1.5f64));

        let output = reduce(input, args, &mut ReportContext::default()).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_reduce_average_mean_float_arrays() {
        let array_1 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]));
        let array_2 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(6f64).into(),
            RadonFloat::from(10f64).into(),
        ]));
        let input = RadonArray::from(vec![array_1, array_2]);

        let expected = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(3.5f64).into(),
            RadonFloat::from(6f64).into(),
        ]));

        let args = &[Value::Integer(0x03)]; // This is RadonReducers::AverageMean
        let output = reduce(&input, args, &mut ReportContext::default()).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_reduce_average_mean_float_arrays_different_size() {
        let array_1 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
            RadonFloat::from(3f64).into(),
        ]));
        let array_2 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(6f64).into(),
            RadonFloat::from(10f64).into(),
        ]));
        let input = RadonArray::from(vec![array_1, array_2]);

        let expected = RadError::DifferentSizeArrays {
            method: "RadonArray::transpose".to_string(),
            first: 3,
            second: 2,
        };

        let args = &[Value::Integer(0x03)]; // This is RadonReducers::AverageMean
        let output = reduce(&input, args, &mut ReportContext::default()).unwrap_err();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_reduce_average_mean_float_array_of_arrays() {
        let array_11 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]));
        let array_12 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(3f64).into(),
            RadonFloat::from(4f64).into(),
        ]));
        let array_13 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(5f64).into(),
            RadonFloat::from(6f64).into(),
        ]));
        let array1 = RadonTypes::from(RadonArray::from(vec![array_11, array_12, array_13]));

        let array_21 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(11f64).into(),
            RadonFloat::from(12f64).into(),
        ]));
        let array_22 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(13f64).into(),
            RadonFloat::from(14f64).into(),
        ]));
        let array_23 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(15f64).into(),
            RadonFloat::from(16f64).into(),
        ]));
        let array2 = RadonTypes::from(RadonArray::from(vec![array_21, array_22, array_23]));
        let input = RadonArray::from(vec![array1, array2]);

        let array_e1 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(6f64).into(),
            RadonFloat::from(7f64).into(),
        ]));
        let array_e2 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(8f64).into(),
            RadonFloat::from(9f64).into(),
        ]));
        let array_e3 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(10f64).into(),
            RadonFloat::from(11f64).into(),
        ]));
        let expected = RadonTypes::from(RadonArray::from(vec![array_e1, array_e2, array_e3]));

        let args = &[Value::Integer(0x03)]; // This is RadonReducers::AverageMean
        let output = reduce(&input, args, &mut ReportContext::default()).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_operate_reduce_mean_empty() {
        let input = RadonArray::from(vec![]);
        let output = mean(&input, MeanReturnPolicy::ReturnFloat).unwrap();
        assert_eq!(output, RadonTypes::from(RadonFloat::from(std::f64::NAN)));
    }

    #[test]
    fn test_reduce_average_mean_integer() {
        let input = &RadonArray::from(vec![
            RadonInteger::from(1i128).into(),
            RadonInteger::from(2i128).into(),
        ]);
        let args = &[Value::Integer(RadonReducers::AverageMean as i128)];
        let expected = RadonTypes::Integer(RadonInteger::from(2));

        let output = reduce(input, args, &mut ReportContext::default()).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_reduce_average_mean_integer_arrays() {
        let array_1 = RadonTypes::from(RadonArray::from(vec![
            RadonInteger::from(1).into(),
            RadonInteger::from(2).into(),
        ]));
        let array_2 = RadonTypes::from(RadonArray::from(vec![
            RadonInteger::from(6).into(),
            RadonInteger::from(10).into(),
        ]));
        let input = RadonArray::from(vec![array_1, array_2]);

        let expected = RadonTypes::from(RadonArray::from(vec![
            RadonInteger::from(4).into(),
            RadonInteger::from(6).into(),
        ]));

        let args = &[Value::Integer(RadonReducers::AverageMean as i128)];
        let output = reduce(&input, args, &mut ReportContext::default()).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_reduce_average_mean_string_unsupported() {
        let input = &RadonArray::from(vec![
            RadonString::from("Hello").into(),
            RadonString::from("world").into(),
        ]);
        let args = &[Value::Integer(0x03)]; // This is RadonReducers::AverageMean
        let output = reduce(input, args, &mut ReportContext::default()).unwrap_err();

        let expected = RadError::UnsupportedReducer {
            array: input.clone(),
            reducer: "RadonReducers::AverageMean".to_string(),
        };

        assert_eq!(output, expected);
    }

    #[test]
    fn test_reduce_average_mean_float_int_arrays() {
        let array_1 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]));
        let array_2 = RadonTypes::from(RadonArray::from(vec![
            RadonInteger::from(6i128).into(),
            RadonInteger::from(10i128).into(),
        ]));
        let input = RadonArray::from(vec![array_1, array_2]);

        let expected = RadError::MismatchingTypes {
            method: RadonReducers::AverageMean.to_string(),
            expected: RadonFloat::radon_type_name(),
            found: RadonInteger::radon_type_name(),
        };

        let args = &[Value::Integer(0x03)]; // This is RadonReducers::AverageMean
        let output = reduce(&input, args, &mut ReportContext::default()).unwrap_err();

        assert_eq!(output, expected);
    }
}
