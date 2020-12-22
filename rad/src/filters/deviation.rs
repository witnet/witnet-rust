use crate::{
    error::RadError,
    filters::RadonFilters,
    operators::array::transpose,
    reducers,
    types::{array::RadonArray, boolean::RadonBoolean, float::RadonFloat, RadonType, RadonTypes},
};
use serde_cbor::Value;
use std::convert::TryFrom;
use witnet_data_structures::radon_report::{ReportContext, Stage};

// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(clippy::cast_precision_loss)]
pub fn standard_filter(
    input: &RadonArray,
    extra_args: &[Value],
    context: &mut ReportContext<RadonTypes>,
) -> Result<RadonTypes, RadError> {
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
        Value::Integer(i) => *i as f64,
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
                    array: input.clone(),
                    filter: RadonFilters::DeviationStandard.to_string(),
                });
            }
            let bool_matrix = boolean_standard_filter(&input, sigmas_float)?;

            Ok(keep_rows(input, &bool_matrix, context))
        }
        Some(_rad_types) => {
            // 1D array
            let bool_array = boolean_standard_filter(&input, sigmas_float)?;

            let bool_vec: Vec<bool> = bool_array
                .value()
                .iter()
                .map(|b| match b {
                    RadonTypes::Boolean(rad_bool) => !rad_bool.value(),
                    _ => panic!("Expected RadonArray of RadonBoolean"),
                })
                .collect();

            let mut result = vec![];
            for (item, &b) in input.value().into_iter().zip(bool_vec.iter()) {
                if !b {
                    result.push(item);
                }
            }

            if let Stage::Tally(ref mut metadata) = context.stage {
                metadata.update_liars(bool_vec);
            }

            Ok(RadonArray::from(result).into())
        }
    }
}

// Only keep rows from input for which all values in keep are true.
// input and keep are assumed to have the same dimension
fn keep_rows(
    input: &RadonArray,
    keep: &RadonArray,
    context: &mut ReportContext<RadonTypes>,
) -> RadonTypes {
    let mut result = vec![];
    let mut bool_vec = vec![];
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
        bool_vec.push(!row_true);
    }

    if let Stage::Tally(ref mut metadata) = context.stage {
        metadata.update_liars(bool_vec);
    }

    RadonTypes::from(RadonArray::from(result))
}

// Return an array with the same dimensions as the input, with a boolean indicating
// whether to keep a value or not
// FIXME: Allow for now, since there is no safe cast function from an i128 to float yet
#[allow(clippy::cast_precision_loss)]
fn boolean_standard_filter(input: &RadonArray, sigmas_float: f64) -> Result<RadonArray, RadError> {
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
            let mean =
                reducers::average::mean(input, reducers::average::MeanReturnPolicy::ReturnFloat)?;
            let mean_float = RadonFloat::try_from(mean)?;
            let std_dev = reducers::deviation::standard(input)?;
            let std_dev_float = RadonFloat::try_from(std_dev)?;

            let (keep_min, keep_max) =
                standard_limits(mean_float.value(), std_dev_float.value(), sigmas_float);

            let mut result = vec![];
            for item in input.value() {
                let x = RadonFloat::try_from(item.clone())?;
                let xv = x.value();
                let keep = xv >= keep_min && xv <= keep_max;
                result.push(RadonTypes::Boolean(RadonBoolean::from(keep)));
            }

            Ok(RadonArray::from(result))
        }
        Some(RadonTypes::Integer(_)) => {
            let mean =
                reducers::average::mean(input, reducers::average::MeanReturnPolicy::ReturnFloat)?;
            let mean_float = RadonFloat::try_from(mean)?;
            let std_dev = reducers::deviation::standard(input)?;
            let std_dev_float = RadonFloat::try_from(std_dev)?;

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
        Some(_rad_types) => Err(RadError::UnsupportedFilter {
            array: input.clone(),
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
    use witnet_data_structures::radon_report::TallyMetaData;

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

        let result = standard_filter(&input, &extra_args, &mut ReportContext::default());

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

        let result = standard_filter(&input, &extra_args, &mut ReportContext::default());

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
            array: input.clone(),
            filter: RadonFilters::DeviationStandard.to_string(),
        };

        let result = standard_filter(&input, &extra_args, &mut ReportContext::default());

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

        let mut context = ReportContext {
            stage: Stage::Tally(TallyMetaData::default()),
            ..ReportContext::default()
        };

        let output = standard_filter(&input, &extra_args, &mut context).unwrap();

        assert_eq!(output, expected);

        if let Stage::Tally(metadata) = context.stage {
            assert_eq!(metadata.liars, vec![false, false, true]);
        } else {
            panic!("Not tally stage");
        }
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

        let mut context = ReportContext {
            stage: Stage::Tally(TallyMetaData::default()),
            ..ReportContext::default()
        };

        let output = standard_filter(&input, &extra_args, &mut context).unwrap();

        assert_eq!(output, expected);

        if let Stage::Tally(metadata) = context.stage {
            assert_eq!(metadata.liars, vec![false, false, true]);
        } else {
            panic!("Not tally stage");
        }
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

        let mut context = ReportContext {
            stage: Stage::Tally(TallyMetaData::default()),
            ..ReportContext::default()
        };

        let output = standard_filter(&input, &extra_args, &mut context).unwrap();

        assert_eq!(output, expected);

        if let Stage::Tally(metadata) = context.stage {
            assert_eq!(metadata.liars, vec![false, true, true]);
        } else {
            panic!("Not tally stage");
        }
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

        let mut context = ReportContext {
            stage: Stage::Tally(TallyMetaData::default()),
            ..ReportContext::default()
        };

        let output = standard_filter(&input, &extra_args, &mut context).unwrap();

        assert_eq!(output, expected);

        if let Stage::Tally(metadata) = context.stage {
            assert_eq!(metadata.liars, vec![true, true, true]);
        } else {
            panic!("Not tally stage");
        }
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
        let tt = RadonTypes::Array(input);
        let input = RadonArray::from(vec![tt; 3]);
        let extra_args = vec![Value::Float(1.0)];

        let expected = RadError::UnsupportedFilter {
            array: input.clone(),
            filter: RadonFilters::DeviationStandard.to_string(),
        };

        let result = standard_filter(&input, &extra_args, &mut ReportContext::default());

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

        let output = standard_filter(&input, &extra_args, &mut ReportContext::default())?;

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

        let output = standard_filter(&input, &extra_args, &mut ReportContext::default())?;

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

    #[test]
    fn test_filter_deviation_standard_integer_three_int_sigma() {
        let input = vec![1, 2, 3];
        let sigmas = 1;
        let expected = vec![2];

        let input_vec: Vec<RadonTypes> = input
            .iter()
            .map(|f| RadonTypes::Integer(RadonInteger::from(*f)))
            .collect();
        let input = RadonArray::from(input_vec);
        let extra_args = vec![Value::Integer(sigmas)];

        let output = standard_filter(&input, &extra_args, &mut ReportContext::default()).unwrap();

        let output_vec = match output {
            RadonTypes::Array(x) => x.value(),
            _ => panic!("Filter method should return a RadonArray"),
        };
        let output_i128: Vec<i128> = output_vec
            .into_iter()
            .map(|r| match r {
                RadonTypes::Integer(x) => x.value(),
                _ => panic!("Filter method should return an array of integers"),
            })
            .collect();

        assert_eq!(output_i128, expected);
    }
}
