use crate::{
    error::RadError,
    operators::array::transpose,
    reducers::{
        average::{mean, MeanReturnPolicy},
        RadonReducers,
    },
    types::{array::RadonArray, float::RadonFloat, RadonType, RadonTypes},
};
use std::ops::Div;

/// Population standard deviation
pub fn standard(input: &RadonArray) -> Result<RadonTypes, RadError> {
    let value = input.value();
    let value_len = value.len();

    match value.first() {
        None => Ok(RadonTypes::from(RadonFloat::from(std::f64::NAN))),
        Some(RadonTypes::Float(_)) => {
            let mean_value = mean(input, MeanReturnPolicy::ReturnFloat)?;
            let mean_float = if let RadonTypes::Float(f) = mean_value {
                f
            } else {
                unreachable!()
            };

            let sum_of_deviations_squared =
                value.iter().try_fold(0f64, |sum, item| match item {
                    RadonTypes::Float(f64_value) => {
                        let deviation = f64_value.value() - mean_float.value();
                        let deviation_squared = deviation * deviation;

                        Ok(sum + deviation_squared)
                    }
                    _ => Err(RadError::MismatchingTypes {
                        method: RadonReducers::AverageMean.to_string(),
                        expected: RadonFloat::radon_type_name(),
                        found: item.clone().radon_type_name(),
                    }),
                })?;

            let variance: f64 = sum_of_deviations_squared.div(value_len as f64);
            let stddev = variance.sqrt();

            Ok(RadonTypes::from(RadonFloat::from(stddev)))
        }
        Some(RadonTypes::Integer(_)) => {
            let mean_value = mean(input, MeanReturnPolicy::ReturnFloat)?;
            let mean_float = if let RadonTypes::Float(f) = mean_value {
                f
            } else {
                unreachable!()
            };

            let sum_of_deviations_squared =
                value.iter().try_fold(0f64, |sum, item| match item {
                    RadonTypes::Integer(i128_value) => {
                        let deviation = i128_value.value() as f64 - mean_float.value();
                        let deviation_squared = deviation * deviation;

                        Ok(sum + deviation_squared)
                    }
                    _ => Err(RadError::MismatchingTypes {
                        method: RadonReducers::AverageMean.to_string(),
                        expected: RadonFloat::radon_type_name(),
                        found: item.clone().radon_type_name(),
                    }),
                })?;

            let variance: f64 = sum_of_deviations_squared.div(value_len as f64);
            let stddev = variance.sqrt();

            Ok(RadonTypes::from(RadonFloat::from(stddev)))
        }
        Some(RadonTypes::Array(_)) => {
            let v = transpose(input)?;

            let mut mean_v = vec![];
            for v2std in v.value() {
                if let RadonTypes::Array(v2std) = v2std {
                    mean_v.push(standard(&v2std)?);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{float::RadonFloat, integer::RadonInteger, string::RadonString};
    use std::convert::TryFrom;

    #[test]
    fn test_reduce_deviation_standard_float() {
        let input = &RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]);
        let expected = RadonTypes::from(RadonFloat::from(0.5));

        let output = standard(input).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_reduce_deviation_standard_float2() {
        let input = &RadonArray::from(vec![
            RadonFloat::from(100f64).into(),
            RadonFloat::from(256f64).into(),
            RadonFloat::from(1003f64).into(),
            RadonFloat::from(134f64).into(),
            RadonFloat::from(200f64).into(),
            RadonFloat::from(87f64).into(),
        ]);

        let expected = 321u32;

        let output = standard(input).unwrap();
        let output = RadonFloat::try_from(output).unwrap();

        assert_eq!(output.value().round() as u32, expected);
    }

    #[test]
    fn test_reduce_deviation_standard_float_arrays() {
        let array_1 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]));
        let array_2 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(2f64).into(),
            RadonFloat::from(4f64).into(),
        ]));
        let input = RadonArray::from(vec![array_1, array_2]);

        let expected = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(0.5).into(),
            RadonFloat::from(1.0).into(),
        ]));

        let output = standard(&input).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_reduce_deviation_standard_float_arrays_different_size() {
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

        let result = standard(&input);

        assert_eq!(result.unwrap_err(), expected);
    }

    #[test]
    fn test_reduce_deviation_standard_float_array_of_arrays() {
        let array_11 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(7f64).into(),
        ]));
        let array_12 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(3f64).into(),
            RadonFloat::from(9f64).into(),
        ]));
        let array_13 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(5f64).into(),
            RadonFloat::from(11f64).into(),
        ]));
        let array1 = RadonTypes::from(RadonArray::from(vec![array_11, array_12, array_13]));

        let array_21 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(2f64).into(),
            RadonFloat::from(8f64).into(),
        ]));
        let array_22 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(4f64).into(),
            RadonFloat::from(10f64).into(),
        ]));
        let array_23 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(6f64).into(),
            RadonFloat::from(12f64).into(),
        ]));
        let array2 = RadonTypes::from(RadonArray::from(vec![array_21, array_22, array_23]));
        let input = RadonArray::from(vec![array1, array2]);

        let array_e1 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(0.5).into(),
            RadonFloat::from(0.5).into(),
        ]));
        let array_e2 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(0.5).into(),
            RadonFloat::from(0.5).into(),
        ]));
        let array_e3 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(0.5).into(),
            RadonFloat::from(0.5).into(),
        ]));
        let expected = RadonTypes::from(RadonArray::from(vec![array_e1, array_e2, array_e3]));

        let output = standard(&input).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_operate_reduce_deviation_standard_empty() {
        let input = RadonArray::from(vec![]);
        let output = standard(&input).unwrap();
        assert_eq!(output, RadonTypes::from(RadonFloat::from(std::f64::NAN)));
    }

    #[test]
    fn test_operate_reduce_deviation_standard_one_element() {
        let input = RadonArray::from(vec![RadonTypes::Float(RadonFloat::from(4f64))]);
        let output = standard(&input).unwrap();
        assert_eq!(output, RadonTypes::from(RadonFloat::from(0f64)));
    }

    #[test]
    fn test_reduce_deviation_standard_integer() {
        let input = &RadonArray::from(vec![
            RadonInteger::from(1i128).into(),
            RadonInteger::from(2i128).into(),
        ]);
        let expected = RadonTypes::from(RadonFloat::from(0.5));

        let output = standard(&input).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_reduce_deviation_standard_integer_arrays() {
        let array_1 = RadonTypes::from(RadonArray::from(vec![
            RadonInteger::from(1i128).into(),
            RadonInteger::from(6i128).into(),
        ]));
        let array_2 = RadonTypes::from(RadonArray::from(vec![
            RadonInteger::from(2i128).into(),
            RadonInteger::from(8i128).into(),
        ]));
        let input = RadonArray::from(vec![array_1, array_2]);

        let expected = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(0.5).into(),
            RadonFloat::from(1.0).into(),
        ]));

        let output = standard(&input).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_reduce_deviation_standard_string_unsupported() {
        let input = &RadonArray::from(vec![
            RadonString::from("Hello").into(),
            RadonString::from("world").into(),
        ]);
        let result = standard(&input);

        let expected = RadError::UnsupportedReducer {
            array: input.clone(),
            reducer: "RadonReducers::AverageMean".to_string(),
        };

        assert_eq!(result.unwrap_err(), expected);
    }

    #[test]
    fn test_reduce_deviation_standard_float_int_arrays() {
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

        let result = standard(&input);

        assert_eq!(result.unwrap_err(), expected);
    }
}
