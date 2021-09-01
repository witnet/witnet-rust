use crate::{
    error::RadError,
    operators::array as array_operators,
    reducers::{average::mean, average::MeanReturnPolicy, RadonReducers},
    types::{array::RadonArray, float::RadonFloat, RadonType, RadonTypes},
    ReportContext,
};
use ordered_float::NotNan;

/// The median of a list of length N is the value at position floor(N/2) if N is odd,
/// and the average of the values at positions (N/2 - 1) and (N/2) if N is even.
///
/// The input will be sorted using the ArraySort operator.
/// The input must be an array of RadonIntegers or RadonFloats. Any RadonFloats set to NaN will be
/// ignored for this operation.
pub fn median(input: &RadonArray) -> Result<RadonTypes, RadError> {
    let value = input.value();
    let value_len = value.len();

    match value.first() {
        None => Err(RadError::ModeEmpty),
        Some(RadonTypes::Float(_)) => {
            // Collect non-NaN values into a vector, and sort them
            let mut input_not_nan: Vec<NotNan<f64>> = Vec::with_capacity(value_len);

            for item in value {
                match item {
                    RadonTypes::Float(f64_value) => {
                        if let Ok(not_nan) = NotNan::new(f64_value.value()) {
                            input_not_nan.push(not_nan)
                        }
                    }
                    _ => {
                        return Err(RadError::MismatchingTypes {
                            method: RadonReducers::AverageMedian.to_string(),
                            expected: RadonFloat::radon_type_name(),
                            found: item.radon_type_name(),
                        })
                    }
                }
            }

            input_not_nan.sort();

            if input_not_nan.is_empty() {
                // This can happen if all elements are NaN
                Err(RadError::ModeEmpty)
            } else if input_not_nan.len() % 2 == 1 {
                // Odd number of elements: take element at floor(N/2):
                let median_pos = input_not_nan.len() / 2;
                let median_elem = input_not_nan[median_pos].into_inner();

                Ok(RadonTypes::Float(RadonFloat::from(median_elem)))
            } else {
                // Even number of elements: take average of element at (N/2 - 1) and N/2
                let right_pos = input_not_nan.len() / 2;
                let right_elem = input_not_nan[right_pos].into_inner();
                let left_pos = right_pos - 1;
                let left_elem = input_not_nan[left_pos].into_inner();

                // Create new array to be able to use average::mean reducer
                let rl = RadonArray::from(vec![
                    RadonTypes::Float(RadonFloat::from(left_elem)),
                    RadonTypes::Float(RadonFloat::from(right_elem)),
                ]);
                // MeanReturnPolicy only applies to integers, so this will actually return a float
                mean(&rl, MeanReturnPolicy::RoundToInteger)
            }
        }
        Some(RadonTypes::Integer(_)) => {
            let sorted_input =
                match array_operators::sort(input, &[], &mut ReportContext::default()) {
                    Ok(RadonTypes::Array(arr)) => arr.value(),
                    Ok(_different_type) => unreachable!(),
                    Err(e) => return Err(e),
                };

            if sorted_input.is_empty() {
                // This is unreachable
                Err(RadError::ModeEmpty)
            } else if sorted_input.len() % 2 == 1 {
                // Odd number of elements: take element at floor(N/2):
                let median_pos = sorted_input.len() / 2;

                Ok(sorted_input[median_pos].clone())
            } else {
                // Even number of elements: take average of element at (N/2 - 1) and N/2
                let right_pos = sorted_input.len() / 2;
                let left_pos = right_pos - 1;

                // Create new array to be able to use average::mean reducer
                let rl = RadonArray::from(vec![
                    sorted_input[left_pos].clone(),
                    sorted_input[right_pos].clone(),
                ]);
                // RoundToInteger means that when the average is not an integer, it will be rounded to an
                // integer. For example, the average of 1 and 2, which is 1.5, will be rounded to 2.
                mean(&rl, MeanReturnPolicy::RoundToInteger)
            }
        }
        Some(_rad_types) => Err(RadError::UnsupportedReducer {
            array: input.clone(),
            reducer: RadonReducers::AverageMedian.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        types::{float::RadonFloat, integer::RadonInteger, string::RadonString},
        RadError::ModeEmpty,
    };

    #[test]
    fn test_operate_reduce_median_empty() {
        let input = RadonArray::from(vec![]);
        let output = median(&input).unwrap_err();
        let expected_error = ModeEmpty;
        assert_eq!(output, expected_error);
    }

    #[test]
    fn test_operate_reduce_median_float_odd() {
        let input = RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
            RadonFloat::from(2f64).into(),
        ]);
        let expected = RadonTypes::from(RadonFloat::from(2f64));
        let output = median(&input).unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_operate_reduce_median_float_even() {
        let input = RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]);

        let expected = RadonTypes::from(RadonFloat::from(1.5f64));
        let output = median(&input).unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_operate_reduce_median_float_with_nans() {
        let input = RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
            RadonFloat::from(f64::NAN).into(),
        ]);

        let expected = RadonTypes::from(RadonFloat::from(1.5f64));
        let output = median(&input).unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_operate_reduce_median_float_all_nans() {
        let input = RadonArray::from(vec![
            RadonFloat::from(f64::NAN).into(),
            RadonFloat::from(f64::NAN).into(),
        ]);

        let output = median(&input).unwrap_err();
        let expected_error = ModeEmpty;
        assert_eq!(output, expected_error);
    }

    #[test]
    fn test_operate_reduce_median_int_odd() {
        let input = RadonArray::from(vec![
            RadonInteger::from(1i128).into(),
            RadonInteger::from(2i128).into(),
            RadonInteger::from(2i128).into(),
        ]);
        let expected = RadonTypes::from(RadonInteger::from(2i128));
        let output = median(&input).unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_operate_reduce_median_int_even() {
        // The median should be 1.5, but it is rounded to 2
        let input = RadonArray::from(vec![
            RadonInteger::from(1i128).into(),
            RadonInteger::from(2i128).into(),
        ]);
        let output = median(&input).unwrap();
        let expected = RadonTypes::from(RadonInteger::from(2i128));
        assert_eq!(output, expected);
    }

    #[test]
    fn test_operate_reduce_median_int_unsorted_input() {
        let input = RadonArray::from(vec![
            RadonInteger::from(1i128).into(),
            RadonInteger::from(1i128).into(),
            RadonInteger::from(5i128).into(),
            RadonInteger::from(5i128).into(),
            RadonInteger::from(3i128).into(),
        ]);
        let expected = RadonTypes::from(RadonInteger::from(3i128));
        let output = median(&input).unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_operate_reduce_median_str_odd() {
        let input = RadonArray::from(vec![
            RadonString::from("Bye world!").into(),
            RadonString::from("Hello world!").into(),
            RadonString::from("Hello world!").into(),
        ]);
        let output = median(&input).unwrap_err();
        let expected_error = RadError::UnsupportedReducer {
            array: input,
            reducer: "RadonReducers::AverageMedian".to_string(),
        };
        assert_eq!(output, expected_error);
    }

    #[test]
    fn test_operate_reduce_median_str_even() {
        let input = RadonArray::from(vec![
            RadonString::from("Bye world!").into(),
            RadonString::from("Hello world!").into(),
        ]);
        let output = median(&input).unwrap_err();
        let expected_error = RadError::UnsupportedReducer {
            array: input,
            reducer: "RadonReducers::AverageMedian".to_string(),
        };
        assert_eq!(output, expected_error);
    }

    #[test]
    fn test_operate_reduce_median_array() {
        let array1 = RadonArray::from(vec![
            RadonInteger::from(1i128).into(),
            RadonInteger::from(2i128).into(),
            RadonInteger::from(3i128).into(),
        ]);
        let array2 = RadonArray::from(vec![
            RadonInteger::from(2i128).into(),
            RadonInteger::from(5i128).into(),
            RadonInteger::from(4i128).into(),
        ]);
        let array3 = RadonArray::from(vec![
            RadonInteger::from(1i128).into(),
            RadonInteger::from(2i128).into(),
            RadonInteger::from(3i128).into(),
            RadonInteger::from(4i128).into(),
        ]);

        let input = RadonArray::from(vec![
            array1.clone().into(),
            array1.clone().into(),
            array1.into(),
            array2.into(),
            array3.into(),
        ]);

        let output = median(&input).unwrap_err();
        let expected_error = RadError::UnsupportedReducer {
            array: input,
            reducer: "RadonReducers::AverageMedian".to_string(),
        };
        assert_eq!(output, expected_error);
    }

    #[test]
    fn test_median_big_number() {
        let input = RadonArray::from(vec![
            RadonInteger::from(18446744073709551616).into(),
            RadonInteger::from(18446744073709551616).into(),
            RadonInteger::from(2).into(),
        ]);

        let expected = RadonTypes::from(RadonInteger::from(18446744073709551616));
        let output = median(&input).unwrap();
        assert_eq!(output, expected);
    }
}
