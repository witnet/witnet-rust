use crate::{
    filters::RadonFilters,
    operators::array::transpose,
    rad_error::RadError,
    reducers,
    types::{array::RadonArray, boolean::RadonBoolean, RadonType, RadonTypes},
};
use serde_cbor::Value;

pub fn standard_filter(input: &RadonArray, extra_args: &[Value]) -> Result<RadonTypes, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonArray::radon_type_name(),
        operator: RadonFilters::DeviationStandard.to_string(),
        args: extra_args.to_vec(),
    };

    if extra_args.len() != 1 {
        return Err(wrong_args());
    }

    let sigmas = &extra_args[0];
    let sigmas_float = match sigmas {
        Value::Float(f) => *f,
        _ => {
            return Err(wrong_args());
        }
    };

    let value = input.value();

    match value.first() {
        None => Ok(RadonTypes::from(input.clone())),
        Some(RadonTypes::Array(arr2)) => {
            // 2D array
            if let Some(RadonTypes::Array(_arr3)) = arr2.value().first() {
                // 3D array
                return Err(RadError::UnsupportedFilter {
                    inner_type: "RadonArray<RadonArray<RadonArray>>>".to_string(),
                    filter: RadonFilters::DeviationStandard.to_string(),
                });
            }
            let bool_matrix = boolean_standard_filter(&input, sigmas_float)?;

            keep_rows(input, &bool_matrix)
        }
        Some(_rad_types) => {
            // 1D array
            let bool_array = boolean_standard_filter(&input, sigmas_float)?;

            let mut result = vec![];
            for (item, keep) in input.value().into_iter().zip(bool_array.value()) {
                let keep = if let RadonTypes::Boolean(b) = keep {
                    b.value()
                } else {
                    panic!("Expected RadonArray of RadonBoolean");
                };
                if keep {
                    result.push(item);
                }
            }

            Ok(RadonArray::from(result).into())
        }
    }
}

// Only keep rows from input for which all values in keep are true.
// input and keep are assumed to have the same dimension
fn keep_rows(input: &RadonArray, keep: &RadonArray) -> Result<RadonTypes, RadError> {
    let mut result = vec![];
    for (item, keep_array) in input.value().into_iter().zip(keep.value()) {
        let keep_array = if let RadonTypes::Array(a) = keep_array {
            a
        } else {
            panic!("Expected 2D array");
        };

        let row_true = keep_array.value().iter().all(|x| {
            if let RadonTypes::Boolean(b) = x {
                b.value()
            } else {
                panic!("Only 2D arrays are supported");
            }
        });

        if row_true {
            result.push(item);
        }
    }

    Ok(RadonTypes::from(RadonArray::from(result)))
}

// Return an array with the same dimensions as the input, with a boolean indicating
// whether to keep a value or not
fn boolean_standard_filter(input: &RadonArray, sigmas_float: f64) -> Result<RadonArray, RadError> {
    let assume_float = |x| {
        if let RadonTypes::Float(f) = x {
            f
        } else {
            unreachable!()
        }
    };

    // if input is empty, return the array
    if input.value().is_empty() {
        return Ok(input.clone());
    }

    if !input.is_homogeneous() {
        return Err(RadError::UnsupportedOpNonHomogeneous {
            operator: RadonFilters::DeviationStandard.to_string(),
        });
    }

    let value = input.value();

    match value.first() {
        None => Ok(input.clone()),
        Some(RadonTypes::Float(_)) => {
            let mean = reducers::average::mean(input)?;
            let mean_float = assume_float(mean);
            let std_dev = reducers::deviation::standard(input)?;
            let std_dev_float = assume_float(std_dev);

            let (keep_min, keep_max) =
                standard_limits(mean_float.value(), std_dev_float.value(), sigmas_float);

            let mut result = vec![];
            for item in input.value() {
                let x = assume_float(item.clone());
                let xv = x.value();
                let keep = xv >= keep_min && xv <= keep_max;
                result.push(RadonTypes::Boolean(RadonBoolean::from(keep)));
            }

            Ok(RadonArray::from(result))
        }
        Some(RadonTypes::Integer(_)) => {
            let mean = reducers::average::mean(input)?;
            let mean_float = assume_float(mean);
            let std_dev = reducers::deviation::standard(input)?;
            let std_dev_float = assume_float(std_dev);

            let (keep_min, keep_max) =
                standard_limits(mean_float.value(), std_dev_float.value(), sigmas_float);

            let mut result = vec![];
            for item in input.value() {
                let xv = if let RadonTypes::Integer(i) = item {
                    i.value() as f64
                } else {
                    unreachable!()
                };
                let keep = xv >= keep_min && xv <= keep_max;
                result.push(RadonTypes::Boolean(RadonBoolean::from(keep)));
            }

            Ok(RadonArray::from(result))
        }
        Some(RadonTypes::Array(_)) => {
            let v = transpose(input)?;

            let mut standard_v = vec![];
            for v2mean in v.value() {
                if let RadonTypes::Array(v2mean) = v2mean {
                    standard_v.push(RadonTypes::from(boolean_standard_filter(
                        &v2mean,
                        sigmas_float,
                    )?));
                } else {
                    unreachable!()
                }
            }

            let o = RadonArray::from(standard_v);
            let ot = transpose(&o)?;

            Ok(ot)
        }
        Some(rad_types) => Err(RadError::UnsupportedFilter {
            inner_type: rad_types.clone().radon_type_name(),
            filter: RadonFilters::DeviationStandard.to_string(),
        }),
    }
}

fn standard_limits(mean: f64, std_dev: f64, sigmas: f64) -> (f64, f64) {
    // Keep values between
    // [mean - sigmas * std_dev, mean + sigmas * std_dev] (inclusive)
    let keep_min = mean - sigmas * std_dev;
    let keep_max = mean + sigmas * std_dev;

    (keep_min, keep_max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{float::RadonFloat, integer::RadonInteger, string::RadonString};
    use std::f64;

    #[test]
    fn test_filter_deviation_standard_no_arg() {
        let input_f64 = &[1.0, 2.0];
        let input_vec: Vec<RadonTypes> = input_f64
            .iter()
            .map(|f| RadonTypes::Float(RadonFloat::from(*f)))
            .collect();
        let input = RadonArray::from(input_vec);
        let extra_args = vec![];

        let expected = RadError::WrongArguments {
            input_type: RadonArray::radon_type_name(),
            operator: RadonFilters::DeviationStandard.to_string(),
            args: extra_args.clone(),
        };

        let result = standard_filter(&input, &extra_args);

        assert_eq!(result.unwrap_err(), expected);
    }

    #[test]
    fn test_filter_deviation_standard_wrong_arg() {
        let input_f64 = &[1.0, 2.0];
        let input_vec: Vec<RadonTypes> = input_f64
            .iter()
            .map(|f| RadonTypes::Float(RadonFloat::from(*f)))
            .collect();
        let input = RadonArray::from(input_vec);
        let extra_args = vec![Value::Text("1.0".to_string())];

        let expected = RadError::WrongArguments {
            input_type: RadonArray::radon_type_name(),
            operator: RadonFilters::DeviationStandard.to_string(),
            args: extra_args.clone(),
        };

        let result = standard_filter(&input, &extra_args);

        assert_eq!(result.unwrap_err(), expected);
    }

    #[test]
    fn test_filter_deviation_standard_unsupported_type() {
        let input_vec: Vec<RadonTypes> = vec![
            RadonTypes::String(RadonString::from("foo")),
            RadonTypes::String(RadonString::from("bar")),
        ];
        let input = RadonArray::from(input_vec);
        let extra_args = vec![Value::Float(1.0)];

        let expected = RadError::UnsupportedFilter {
            inner_type: RadonString::radon_type_name(),
            filter: RadonFilters::DeviationStandard.to_string(),
        };

        let result = standard_filter(&input, &extra_args);

        assert_eq!(result.unwrap_err(), expected);
    }

    // Create a RadonArray of RadonFloats
    fn rfa(f: &[f64]) -> RadonArray {
        let v: Vec<_> = f
            .iter()
            .map(|f| RadonTypes::Float(RadonFloat::from(*f)))
            .collect();

        RadonArray::from(v)
    }

    #[test]
    fn test_filter_deviation_standard_array_of_floats1() {
        let array1 = rfa(&[1.0, 11.0, 21.0]);
        let array2 = rfa(&[2.0, 12.0, 22.0]);
        let array3 = rfa(&[103.0, 113.0, 123.0]);
        let input_vec: Vec<RadonTypes> = vec![
            RadonTypes::Array(array1),
            RadonTypes::Array(array2),
            RadonTypes::Array(array3),
        ];
        let input = RadonArray::from(input_vec);
        let extra_args = vec![Value::Float(1.0)];

        let expected1 = rfa(&[1.0, 11.0, 21.0]);
        let expected2 = rfa(&[2.0, 12.0, 22.0]);
        let expected_vec: Vec<RadonTypes> =
            vec![RadonTypes::Array(expected1), RadonTypes::Array(expected2)];
        let expected = RadonTypes::Array(RadonArray::from(expected_vec));

        let output = standard_filter(&input, &extra_args).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_filter_deviation_standard_array_of_floats2() {
        let array1 = rfa(&[1.0, 12.0, 21.0]);
        let array2 = rfa(&[2.0, 12.0, 22.0]);
        let array3 = rfa(&[103.0, 12.0, 123.0]);
        let input_vec: Vec<RadonTypes> = vec![
            RadonTypes::Array(array1),
            RadonTypes::Array(array2),
            RadonTypes::Array(array3),
        ];
        let input = RadonArray::from(input_vec);
        let extra_args = vec![Value::Float(1.0)];

        let expected1 = rfa(&[1.0, 12.0, 21.0]);
        let expected2 = rfa(&[2.0, 12.0, 22.0]);

        let expected_vec: Vec<RadonTypes> =
            vec![RadonTypes::Array(expected1), RadonTypes::Array(expected2)];
        let expected = RadonTypes::Array(RadonArray::from(expected_vec));

        let output = standard_filter(&input, &extra_args).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_filter_deviation_standard_array_of_floats3() {
        let array1 = rfa(&[1.0, 11.0, 21.0]);
        let array2 = rfa(&[2.0, 112.0, 22.0]);
        let array3 = rfa(&[103.0, 13.0, 123.0]);
        let input_vec: Vec<RadonTypes> = vec![
            RadonTypes::Array(array1),
            RadonTypes::Array(array2),
            RadonTypes::Array(array3),
        ];
        let input = RadonArray::from(input_vec);
        let extra_args = vec![Value::Float(1.0)];

        let expected1 = rfa(&[1.0, 11.0, 21.0]);

        let expected_vec: Vec<RadonTypes> = vec![RadonTypes::Array(expected1)];
        let expected = RadonTypes::Array(RadonArray::from(expected_vec));

        let output = standard_filter(&input, &extra_args).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_filter_deviation_standard_array_of_floats4() {
        let array1 = rfa(&[1.0, 11.0, 21.0]);
        let array2 = rfa(&[2.0, 112.0, 122.0]);
        let array3 = rfa(&[103.0, 13.0, 123.0]);
        let input_vec: Vec<RadonTypes> = vec![
            RadonTypes::Array(array1),
            RadonTypes::Array(array2),
            RadonTypes::Array(array3),
        ];
        let input = RadonArray::from(input_vec);
        let extra_args = vec![Value::Float(1.0)];

        let expected_vec: Vec<RadonTypes> = vec![];
        let expected = RadonTypes::Array(RadonArray::from(expected_vec));

        let output = standard_filter(&input, &extra_args).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_filter_deviation_standard_3d_array() {
        let array1 = rfa(&[1.0, 11.0, 21.0]);
        let array2 = rfa(&[2.0, 112.0, 122.0]);
        let array3 = rfa(&[103.0, 13.0, 123.0]);
        let input_vec: Vec<RadonTypes> = vec![
            RadonTypes::Array(array1),
            RadonTypes::Array(array2),
            RadonTypes::Array(array3),
        ];
        let input = RadonArray::from(input_vec);
        let tt = RadonTypes::Array(input.clone());
        let input = RadonArray::from(vec![tt; 3]);
        let extra_args = vec![Value::Float(1.0)];

        let expected = RadError::UnsupportedFilter {
            inner_type: "RadonArray<RadonArray<RadonArray>>>".to_string(),
            filter: RadonFilters::DeviationStandard.to_string(),
        };

        let result = standard_filter(&input, &extra_args);

        assert_eq!(result.unwrap_err(), expected);
    }

    // Helper function which works with Rust floats, to remove RadonTypes from tests
    fn fstd(input_f64: &[f64], sigmas: f64) -> Result<Vec<f64>, RadError> {
        let input_vec: Vec<RadonTypes> = input_f64
            .iter()
            .map(|f| RadonTypes::Float(RadonFloat::from(*f)))
            .collect();
        let input = RadonArray::from(input_vec);
        let extra_args = vec![Value::Float(sigmas)];

        let output = standard_filter(&input, &extra_args)?;

        let output_vec = match output {
            RadonTypes::Array(x) => x.value(),
            _ => panic!("Filter method should return a RadonArray"),
        };
        let output_f64 = output_vec
            .into_iter()
            .map(|r| match r {
                RadonTypes::Float(x) => x.value(),
                _ => panic!("Filter method should return an array of floats"),
            })
            .collect();

        Ok(output_f64)
    }

    // Helper function which works with Rust integers, to remove RadonTypes from tests
    fn istd(input_i128: &[i128], sigmas: f64) -> Result<Vec<i128>, RadError> {
        let input_vec: Vec<RadonTypes> = input_i128
            .iter()
            .map(|f| RadonTypes::Integer(RadonInteger::from(*f)))
            .collect();
        let input = RadonArray::from(input_vec);
        let extra_args = vec![Value::Float(sigmas)];

        let output = standard_filter(&input, &extra_args)?;

        let output_vec = match output {
            RadonTypes::Array(x) => x.value(),
            _ => panic!("Filter method should return a RadonArray"),
        };
        let output_i128 = output_vec
            .into_iter()
            .map(|r| match r {
                RadonTypes::Integer(x) => x.value(),
                _ => panic!("Filter method should return an array of integers"),
            })
            .collect();

        Ok(output_i128)
    }

    #[test]
    fn test_filter_deviation_standard_empty() {
        let input = vec![];
        let sigmas = 1.0;
        let expected = input.clone();

        assert_eq!(fstd(&input, sigmas), Ok(expected));
    }

    #[test]
    fn test_filter_deviation_standard_float_one() {
        let input = vec![1.0];
        let sigmas = 1.0;
        let expected = input.clone();

        assert_eq!(fstd(&input, sigmas), Ok(expected));
    }

    #[test]
    fn test_filter_deviation_standard_float_two() {
        let input = vec![1.0, 2.0];
        let sigmas = 1.0;
        let expected = input.clone();

        assert_eq!(fstd(&input, sigmas), Ok(expected));
    }

    #[test]
    fn test_filter_deviation_standard_float_two_empty() {
        let input = vec![1.0, 2.0];
        let sigmas = 0.5;
        let expected = vec![];

        assert_eq!(fstd(&input, sigmas), Ok(expected));
    }

    #[test]
    fn test_filter_deviation_standard_float_three() {
        let input = vec![1.0, 2.0, 3.0];
        let sigmas = 1.0;
        let expected = vec![2.0];

        assert_eq!(fstd(&input, sigmas), Ok(expected));
    }

    #[test]
    fn test_filter_deviation_standard_integer_one() {
        let input = vec![1];
        let sigmas = 1.0;
        let expected = input.clone();

        assert_eq!(istd(&input, sigmas), Ok(expected));
    }

    #[test]
    fn test_filter_deviation_standard_integer_two() {
        let input = vec![1, 2];
        let sigmas = 1.0;
        let expected = input.clone();

        assert_eq!(istd(&input, sigmas), Ok(expected));
    }

    #[test]
    fn test_filter_deviation_standard_integer_two_empty() {
        let input = vec![1, 2];
        let sigmas = 0.5;
        let expected = vec![];

        assert_eq!(istd(&input, sigmas), Ok(expected));
    }

    #[test]
    fn test_filter_deviation_standard_integer_three() {
        let input = vec![1, 2, 3];
        let sigmas = 1.0;
        let expected = vec![2];

        assert_eq!(istd(&input, sigmas), Ok(expected));
    }
}
