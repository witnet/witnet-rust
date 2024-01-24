use std::{
    clone::Clone,
    convert::{TryFrom, TryInto},
};

use serde_cbor::value::{from_value, Value};
use witnet_data_structures::radon_report::ReportContext;

use crate::{
    error::RadError,
    filters::{self, RadonFilters},
    operators::string,
    reducers::{self, RadonReducers},
    script::{
        execute_radon_script, partial_results_extract, unpack_subscript,
        RadonScriptExecutionSettings,
    },
    types::{array::RadonArray, integer::RadonInteger, string::RadonString, RadonType, RadonTypes},
};

pub fn count(input: &RadonArray) -> RadonInteger {
    RadonInteger::from(input.value().len() as i128)
}

pub fn reduce(
    input: &RadonArray,
    args: &[Value],
    context: &mut ReportContext<RadonTypes>,
) -> Result<RadonTypes, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonArray::radon_type_name(),
        operator: "Reduce".to_string(),
        args: args.to_vec(),
    };

    if args.len() != 1 {
        return Err(wrong_args());
    }

    let arg = args[0].to_owned();
    let reducer_integer = from_value::<u8>(arg).map_err(|_| wrong_args())?;
    let reducer_code = RadonReducers::try_from(reducer_integer).map_err(|_| wrong_args())?;

    reducers::reduce(input, reducer_code, context)
}

fn inner_get(input: &RadonArray, args: &[Value]) -> Result<RadonTypes, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonArray::radon_type_name(),
        operator: "Get".to_string(),
        args: args.to_vec(),
    };

    if args.len() != 1 {
        return Err(wrong_args());
    }

    let not_found = |index: usize| RadError::ArrayIndexOutOfBounds {
        index: i32::try_from(index).unwrap(),
    };

    let arg = args[0].to_owned();
    let index = from_value::<i32>(arg).map_err(|_| wrong_args())?;
    let index = usize::try_from(index).map_err(|_| RadError::ArrayIndexOutOfBounds { index })?;

    input
        .value()
        .get(index)
        .map(Clone::clone)
        .ok_or_else(|| not_found(index))
}

/// Try to get any kind of `RadonType` from an entry in the input `RadonArray`, as specified
/// by the first argument, which is used as the index.
pub fn get<O: RadonType<T>, T>(input: &RadonArray, args: &[Value]) -> Result<O, RadError>
where
    T: std::fmt::Debug,
{
    let item = inner_get(input, args)?;
    let original_type = item.radon_type_name();

    item.try_into().map_err(|_| RadError::Decode {
        from: original_type,
        to: O::radon_type_name(),
    })
}

/// Try to get a `RadonFloat` or  `RadonInteger` from an entry in the input `RadonArray`, as
/// specified by the first argument, which is used as the index. Internally does some pre-processing
/// to normalize decimal and thousands separators.
pub fn get_number<O>(input: &RadonArray, args: &[Value]) -> Result<O, RadError>
where
    O: TryFrom<RadonString, Error = RadError>,
{
    get_numeric_string(input, args).and_then(O::try_from)
}

/// Try to get a `RadonTypes` from a position in the input `RadonArray`, as specified by the first
/// argument, which is used as the positional index.
///
/// This simply assumes that the element in that position is a number (i.e., `RadonFloat` or
/// `RadonInteger`). If it is not, it will fail with a `RadError` because of `replace_separators`.
fn get_numeric_string(input: &RadonArray, args: &[Value]) -> Result<RadonString, RadError> {
    let item = get::<RadonString, _>(input, &args[..1])?.value();
    let (thousands_separator, decimal_separator) = string::read_separators_from_args(&args[1..]);

    Ok(RadonString::from(string::replace_separators(
        item,
        thousands_separator,
        decimal_separator,
    )))
}

pub fn join(input: &RadonArray, args: &[Value]) -> Result<RadonTypes, RadError> {
    // Join not applicable if the input array is not homogeneous
    if !input.is_homogeneous() {
        return Err(RadError::UnsupportedOpNonHomogeneous {
            operator: "ArrayJoin".to_string(),
        });
    }
    let separator = if !args.is_empty() {
        from_value::<String>(args[0].to_owned()).unwrap_or_default()
    } else {
        String::from("")
    };
    match input.value().first() {
        Some(RadonTypes::String(_)) => {
            let string_list: Vec<String> = input
                .value()
                .into_iter()
                .map(|item| RadonString::try_from(item).unwrap_or_default().value())
                .collect();
            Ok(RadonTypes::from(RadonString::from(
                string_list.join(separator.as_str()),
            )))
        }
        Some(first_item) => Err(RadError::UnsupportedOperator {
            input_type: first_item.radon_type_name().to_string(),
            operator: "ArrayJoin".to_string(),
            args: Some(args.to_vec()),
        }),
        _ => Err(RadError::EmptyArray),
    }
}

pub fn map(
    input: &RadonArray,
    args: &[Value],
    context: &mut ReportContext<RadonTypes>,
) -> Result<RadonTypes, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonArray::radon_type_name(),
        operator: "Map".to_string(),
        args: args.to_vec(),
    };

    if args.len() != 1 {
        return Err(wrong_args());
    }

    let subscript_err = |e| RadError::Subscript {
        input_type: "RadonArray".to_string(),
        operator: "Map".to_string(),
        inner: Box::new(e),
    };
    let subscript = unpack_subscript(&args[0]).map_err(subscript_err)?;

    let mut reports = vec![];
    let mut results = vec![];

    let settings = RadonScriptExecutionSettings::tailored_to_stage(&context.stage);
    for item in input.value() {
        let report = execute_radon_script(item.clone(), subscript.as_slice(), context, settings)?;

        // If there is an error while mapping, short-circuit and bubble up the error as it comes
        // from the radon script execution
        if let RadonTypes::RadonError(error) = &report.result {
            return Err(error.clone().into_inner());
        }

        results.push(report.result.clone());
        reports.push(report);
    }

    // Extract the partial results from the reports and put them in the execution context if needed
    partial_results_extract(&subscript, &reports, context);

    Ok(RadonArray::from(results).into())
}

pub fn filter(
    input: &RadonArray,
    args: &[Value],
    context: &mut ReportContext<RadonTypes>,
) -> Result<RadonTypes, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonArray::radon_type_name(),
        operator: "Filter".to_string(),
        args: args.to_vec(),
    };

    let unknown_filter = |code| RadError::UnknownFilter { code };

    let first_arg = args.get(0).ok_or_else(wrong_args)?;
    match first_arg {
        Value::Array(_arg) => {
            let subscript_err = |e| RadError::Subscript {
                input_type: "RadonArray".to_string(),
                operator: "Filter".to_string(),
                inner: Box::new(e),
            };
            let subscript = unpack_subscript(first_arg).map_err(subscript_err)?;

            let mut reports = vec![];
            let mut results = vec![];

            let settings = RadonScriptExecutionSettings::tailored_to_stage(&context.stage);
            for item in input.value() {
                let report =
                    execute_radon_script(item.clone(), subscript.as_slice(), context, settings)?;

                // If there is an error while mapping, short-circuit and bubble up the error as it comes
                // from the radon script execution
                if let RadonTypes::RadonError(error) = &report.result {
                    return Err(error.clone().into_inner());
                }

                if let RadonTypes::Boolean(boolean) = &report.result {
                    if boolean.value() {
                        results.push(item.clone());
                    }
                } else {
                    return Err(RadError::ArrayFilterWrongSubscript {
                        value: report.result.to_string(),
                    });
                }

                reports.push(report);
            }

            // Extract the partial results from the reports and put them in the execution context if needed
            partial_results_extract(&subscript, &reports, context);

            Ok(RadonArray::from(results).into())
        }
        Value::Integer(arg) => {
            let filter_code =
                RadonFilters::try_from(u8::try_from(*arg).map_err(|_| unknown_filter(*arg))?)
                    .map_err(|_| unknown_filter(*arg))?;
            let (_args, extra_args) = args.split_at(1);

            filters::filter(input, filter_code, extra_args, context)
        }
        _ => Err(wrong_args()),
    }
}

pub fn pick(
    input: &RadonArray,
    args: &[Value],
    _context: &mut ReportContext<RadonTypes>,
) -> Result<RadonTypes, RadError> {
    let not_found = |index: usize| RadError::ArrayIndexOutOfBounds {
        index: i32::try_from(index).unwrap(),
    };

    let wrong_args = || RadError::WrongArguments {
        input_type: RadonArray::radon_type_name(),
        operator: "Pick".to_string(),
        args: args.to_vec(),
    };

    let mut indexes = vec![];
    if args.len() > 1 {
        return Err(wrong_args());
    } else {
        let first_arg = args.get(0).ok_or_else(wrong_args)?;
        match first_arg {
            Value::Array(values) => {
                for value in values.iter() {
                    let index = from_value::<usize>(value.clone()).map_err(|_| wrong_args())?;
                    indexes.push(index);
                }
            }
            Value::Integer(_) => {
                let index = from_value::<usize>(first_arg.clone()).map_err(|_| wrong_args())?;
                indexes.push(index);
            }
            _ => return Err(wrong_args()),
        };
    }

    let mut output_vec: Vec<RadonTypes> = vec![];
    for index in indexes {
        if let Some(value) = input.value().get(index) {
            output_vec.push(value.clone());
        } else {
            return Err(not_found(index));
        }
    }
    Ok(RadonTypes::from(RadonArray::from(output_vec)))
}

pub fn sort(
    input: &RadonArray,
    args: &[Value],
    context: &mut ReportContext<RadonTypes>,
) -> Result<RadonTypes, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonArray::radon_type_name(),
        operator: "Sort".to_string(),
        args: args.to_vec(),
    };

    if args.len() > 1 {
        return Err(wrong_args());
    }

    let input_value = input.value();
    let empty_array = [Value::Array(vec![])];
    // Sort can be called with an optional argument.
    // If that argument is missing, default to []
    let map_args = if args.is_empty() { &empty_array } else { args };
    let mapped_array = match map(input, map_args, context)? {
        RadonTypes::Array(x) => x,
        RadonTypes::RadonError(error) => {
            if let RadError::UnhandledIntercept {
                inner: Some(super_inner),
                message: _,
            }
            | RadError::UnhandledInterceptV2 {
                inner: Some(super_inner),
            } = error.inner()
            {
                return Err(*super_inner.clone());
            }
            return Err(error.inner().clone());
        }
        _ => unreachable!(),
    };

    let mapped_array_value = mapped_array.value();
    let mut tuple_array: Vec<(&RadonTypes, &RadonTypes)> =
        input_value.iter().zip(mapped_array_value.iter()).collect();
    // if input is empty, return the array
    if input.value().is_empty() {
        return Ok(input.clone().into());
    }
    // Sort not applicable if not homogeneous
    if !input.is_homogeneous() {
        return Err(RadError::UnsupportedOpNonHomogeneous {
            operator: "ArraySort".to_string(),
        });
    }

    // Distinguish depending the type
    match &mapped_array_value.first() {
        Some(RadonTypes::String(_)) => {
            tuple_array.sort_by(|a, b| match (a.1, b.1) {
                (RadonTypes::String(a), RadonTypes::String(b)) => a.cmp(b),
                _ => unreachable!(),
            });
        }
        Some(RadonTypes::Integer(_)) => {
            tuple_array.sort_by(|a, b| match (a.1, b.1) {
                (RadonTypes::Integer(a), RadonTypes::Integer(b)) => a.cmp(b),
                _ => unreachable!(),
            });
        }
        _ => {
            return Err(RadError::UnsupportedSortOp {
                array: input.clone(),
            })
        }
    };

    let result: Vec<_> = tuple_array.into_iter().map(|(a, _)| a.clone()).collect();

    Ok(RadonArray::from(result).into())
}

pub fn transpose(input: &RadonArray) -> Result<RadonArray, RadError> {
    let mut v = vec![];
    let mut prev_len = None;
    for item in input.value() {
        match item {
            RadonTypes::Array(rad_value) => {
                let sub_value = rad_value.value();
                let sub_value_len = sub_value.len();

                match prev_len {
                    None => {
                        for sub_item in rad_value.value().into_iter() {
                            v.push(vec![sub_item]);
                        }
                        prev_len = Some(sub_value_len);
                    }
                    Some(prev_len) => {
                        if prev_len == sub_value_len {
                            for (i, sub_item) in rad_value.value().into_iter().enumerate() {
                                v[i].push(sub_item);
                            }
                        } else {
                            return Err(RadError::DifferentSizeArrays {
                                method: "RadonArray::transpose".to_string(),
                                first: prev_len,
                                second: sub_value_len,
                            });
                        }
                    }
                }
            }
            _ => {
                return Err(RadError::MismatchingTypes {
                    method: "T of RadonArray<T>::transpose".to_string(),
                    expected: RadonArray::radon_type_name(),
                    found: item.radon_type_name(),
                });
            }
        }
    }

    let v: Vec<RadonTypes> = v
        .into_iter()
        .map(RadonArray::from)
        .map(RadonTypes::from)
        .collect();

    Ok(RadonArray::from(v))
}

/// This module was introduced for encapsulating the interim legacy logic before WIP-0024 is
/// introduced, for the sake of maintainability.
///
/// Because RADON scripts are never evaluated for old blocks (e.g. during synchronization), this
/// module can theoretically be removed altogether once WIP-0024 is activated.
pub mod legacy {
    use super::*;
    use crate::types::float::RadonFloat;

    /// Legacy (pre-WIP0024) version of `get::<RadonFloat, _>`.
    pub fn get_float_before_wip0024(
        input: &RadonArray,
        args: &[Value],
    ) -> Result<RadonFloat, RadError> {
        let item = inner_get(input, args)?;
        item.try_into().map_err(|_| RadError::ParseFloat {
            message: "invalid float literal".to_string(),
        })
    }

    /// Legacy (pre-WIP0024) version of `get::<RadonInteger, _>`.
    pub fn get_integer_before_wip0024(
        input: &RadonArray,
        args: &[Value],
    ) -> Result<RadonInteger, RadError> {
        let item = inner_get(input, args)?;
        item.try_into().map_err(|_| RadError::ParseInt {
            message: "invalid digit found in string".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use witnet_data_structures::radon_report::{RetrievalMetadata, Stage};

    use crate::{
        error::RadError,
        operators::{
            Operable,
            RadonOpCodes::{
                self, IntegerGreaterThan, IntegerMultiply, MapGetBoolean, MapGetFloat,
                MapGetInteger, MapGetString,
            },
        },
        types::{
            boolean::RadonBoolean, bytes::RadonBytes, float::RadonFloat, integer::RadonInteger,
            map::RadonMap, RadonTypes,
        },
    };

    use super::*;

    #[test]
    fn test_array_count() {
        let input = &RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]);

        let empty = &RadonArray::from(vec![]);

        assert_eq!(count(input), RadonInteger::from(2));
        assert_eq!(count(empty), RadonInteger::from(0));
    }

    #[test]
    fn test_reduce_no_args() {
        let input = &RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]);
        let args = &[];

        let result = reduce(input, args, &mut ReportContext::default());

        assert_eq!(
            &result.unwrap_err().to_string(),
            "Wrong `RadonArray::Reduce()` arguments: `[]`"
        );
    }

    #[test]
    fn test_reduce_wrong_args() {
        let input = &RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]);
        let args = &[Value::Text(String::from("wrong"))]; // This is RadonReducers::AverageMean

        let result = reduce(input, args, &mut ReportContext::default());

        assert_eq!(
            &result.unwrap_err().to_string(),
            "Wrong `RadonArray::Reduce()` arguments: `[Text(\"wrong\")]`"
        );
    }

    #[test]
    fn test_reduce_unknown_reducer() {
        let input = &RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]);
        let args = &[Value::Integer(-1)]; // This doesn't match any reducer code in RadonReducers

        let result = reduce(input, args, &mut ReportContext::default());

        assert_eq!(
            &result.unwrap_err().to_string(),
            "Wrong `RadonArray::Reduce()` arguments: `[Integer(-1)]`"
        );
    }

    #[test]
    fn test_transpose() {
        let array_1 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
            RadonFloat::from(3f64).into(),
        ]));
        let array_2 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(11f64).into(),
            RadonFloat::from(12f64).into(),
            RadonFloat::from(13f64).into(),
        ]));
        let input = RadonArray::from(vec![array_1, array_2]);

        let v1 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(11f64).into(),
        ]));
        let v2 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(2f64).into(),
            RadonFloat::from(12f64).into(),
        ]));
        let v3 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(3f64).into(),
            RadonFloat::from(13f64).into(),
        ]));
        let expected = RadonArray::from(vec![v1, v2, v3]);

        let output = transpose(&input).unwrap();

        assert_eq!(output, expected);

        // Transposing again will return the original input
        assert_eq!(transpose(&output).unwrap(), input);
    }

    #[test]
    fn test_transpose_array_of_floats() {
        let input = RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
            RadonFloat::from(3f64).into(),
        ]);

        let result = transpose(&input);

        assert_eq!(
            &result.unwrap_err().to_string(),
            "Mismatching types in T of RadonArray<T>::transpose. Expected: RadonArray, found: RadonFloat",
        );
    }

    #[test]
    fn test_transpose_array_of_arrays_or_floats() {
        let array_1 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
            RadonFloat::from(3f64).into(),
        ]));
        let array_2 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(11f64).into(),
            RadonFloat::from(12f64).into(),
            RadonFloat::from(13f64).into(),
        ]));

        let float_1 = RadonTypes::from(RadonFloat::from(21f64));
        let float_2 = RadonTypes::from(RadonFloat::from(22f64));

        let input = RadonArray::from(vec![array_1, array_2, float_1, float_2]);

        let result = transpose(&input);

        assert_eq!(
            &result.unwrap_err().to_string(),
            "Mismatching types in T of RadonArray<T>::transpose. Expected: RadonArray, found: RadonFloat",
        );
    }

    #[test]
    fn test_transpose_array_of_arrays_different_size() {
        let array_1 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
            RadonFloat::from(3f64).into(),
        ]));
        let array_2 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(11f64).into(),
            RadonFloat::from(12f64).into(),
        ]));
        let input = RadonArray::from(vec![array_1, array_2]);

        let result = transpose(&input);

        assert_eq!(
            &result.unwrap_err().to_string(),
            "Arrays to be reduced in RadonArray::transpose have different sizes. 3 != 2",
        );
    }

    #[test]
    fn test_transpose_array_of_empty_array() {
        let array_1 = RadonTypes::from(RadonArray::from(vec![]));
        let input = RadonArray::from(vec![array_1]);

        let output = transpose(&input).unwrap();

        assert_eq!(output, RadonArray::from(vec![]));

        // Transposing again will return the original input
        // This fails
        //assert_eq!(transpose(&output).unwrap(), input);
    }

    #[test]
    fn test_transpose_array_of_two_empty_arrays() {
        let array_1 = RadonTypes::from(RadonArray::from(vec![]));
        let array_2 = array_1.clone();
        let input = RadonArray::from(vec![array_1, array_2]);

        let output = transpose(&input).unwrap();

        assert_eq!(output, RadonArray::from(vec![]));

        // Transposing again will return the original input
        // This fails
        //assert_eq!(transpose(&output).unwrap(), input);
    }

    #[test]
    fn test_transpose_array_of_one_empty_array_and_one_with_items() {
        let array_1 = RadonTypes::from(RadonArray::from(vec![]));
        let array_2 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(11f64).into(),
            RadonFloat::from(12f64).into(),
        ]));
        let input = RadonArray::from(vec![array_1, array_2]);

        let result = transpose(&input);

        assert_eq!(
            result.unwrap_err().to_string(),
            "Arrays to be reduced in RadonArray::transpose have different sizes. 0 != 2"
        );
    }

    #[test]
    fn test_transpose_empty_array() {
        let input = RadonArray::from(vec![]);

        let output = transpose(&input).unwrap();

        assert_eq!(output, RadonArray::from(vec![]));

        // Transposing again will return the original input
        assert_eq!(transpose(&output).unwrap(), input);
    }

    #[test]
    fn test_transpose_one_row() {
        let array_1 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
            RadonFloat::from(3f64).into(),
        ]));
        let input = RadonArray::from(vec![array_1]);

        let v1 = RadonTypes::from(RadonArray::from(vec![RadonFloat::from(1f64).into()]));
        let v2 = RadonTypes::from(RadonArray::from(vec![RadonFloat::from(2f64).into()]));
        let v3 = RadonTypes::from(RadonArray::from(vec![RadonFloat::from(3f64).into()]));
        let expected = RadonArray::from(vec![v1, v2, v3]);

        let output = transpose(&input).unwrap();

        assert_eq!(output, expected);

        // Transposing again will return the original input
        assert_eq!(transpose(&output).unwrap(), input);
    }

    #[test]
    fn test_map_with_partial_results() {
        let input = RadonArray::from(vec![
            RadonInteger::from(2).into(),
            RadonInteger::from(3).into(),
        ]);
        let script = vec![Value::Array(vec![
            Value::Array(vec![
                Value::Integer(IntegerMultiply as i128),
                Value::Integer(2),
            ]),
            Value::Array(vec![
                Value::Integer(IntegerGreaterThan as i128),
                Value::Integer(5),
            ]),
        ])];
        let mut context = ReportContext::from_stage(Stage::Retrieval(RetrievalMetadata::default()));
        map(&input, &script, &mut context).unwrap();

        if let Stage::Retrieval(metadata) = context.stage {
            let expected_partial_results = vec![vec![
                vec![RadonInteger::from(2).into(), RadonInteger::from(3).into()],
                vec![RadonInteger::from(4).into(), RadonInteger::from(6).into()],
                vec![
                    RadonBoolean::from(false).into(),
                    RadonBoolean::from(true).into(),
                ],
            ]];
            assert_eq!(metadata.subscript_partial_results, expected_partial_results);
        }
    }

    #[test]
    fn test_map_integer_greater_than() {
        let input = RadonArray::from(vec![
            RadonInteger::from(2).into(),
            RadonInteger::from(6).into(),
        ]);
        let script = vec![Value::Array(vec![Value::Array(vec![
            Value::Integer(IntegerGreaterThan as i128),
            Value::Integer(4),
        ])])];
        let output = map(&input, &script, &mut ReportContext::default()).unwrap();

        let expected = RadonTypes::Array(RadonArray::from(vec![
            RadonBoolean::from(false).into(),
            RadonBoolean::from(true).into(),
        ]));

        assert_eq!(output, expected)
    }

    #[test]
    fn test_map_not_integer_in_subscript() {
        let input = RadonArray::from(vec![
            RadonInteger::from(2).into(),
            RadonInteger::from(6).into(),
        ]);
        let script = vec![Value::Array(vec![Value::Array(vec![Value::Text(
            "Hello".to_string(),
        )])])];
        let result = map(&input, &script, &mut ReportContext::default());

        let expected_err = RadError::Subscript {
            input_type: "RadonArray".to_string(),
            operator: "Map".to_string(),
            inner: Box::new(RadError::NotIntegerOperator),
        };

        assert_eq!(result.unwrap_err(), expected_err);
    }

    #[test]
    fn test_map_wrong_subscript_format() {
        let input = RadonArray::from(vec![
            RadonInteger::from(2).into(),
            RadonInteger::from(6).into(),
        ]);
        let script = vec![Value::Integer(IntegerGreaterThan as i128)];
        let result = map(&input, &script, &mut ReportContext::default());

        let expected_err = RadError::Subscript {
            input_type: "RadonArray".to_string(),
            operator: "Map".to_string(),
            inner: Box::new(RadError::BadSubscriptFormat {
                value: Value::Integer(IntegerGreaterThan as i128),
            }),
        };

        assert_eq!(result.unwrap_err(), expected_err);
    }

    #[test]
    fn test_map_wrong_no_subscript() {
        let input = RadonArray::from(vec![
            RadonInteger::from(2).into(),
            RadonInteger::from(6).into(),
        ]);
        let script = vec![];
        let result = map(&input, &script, &mut ReportContext::default());

        let expected_err = RadError::WrongArguments {
            input_type: "RadonArray",
            operator: "Map".to_string(),
            args: vec![],
        };

        assert_eq!(result.unwrap_err(), expected_err);
    }

    #[test]
    fn test_map_wrong_number_of_args() {
        let input = RadonArray::from(vec![
            RadonInteger::from(2).into(),
            RadonInteger::from(6).into(),
        ]);
        let script = Value::Array(vec![Value::Array(vec![
            Value::Integer(IntegerGreaterThan as i128),
            Value::Integer(4),
        ])]);
        let args = vec![script, Value::Text("foo".to_string())];
        let result = map(&input, &args, &mut ReportContext::default());

        let expected_err = RadError::WrongArguments {
            input_type: "RadonArray",
            operator: "Map".to_string(),
            args,
        };

        assert_eq!(result.unwrap_err(), expected_err);
    }

    #[test]
    fn test_filter_with_partial_results() {
        let input = RadonArray::from(vec![
            RadonInteger::from(2).into(),
            RadonInteger::from(3).into(),
        ]);
        let script = vec![Value::Array(vec![
            Value::Array(vec![
                Value::Integer(IntegerMultiply as i128),
                Value::Integer(2),
            ]),
            Value::Array(vec![
                Value::Integer(IntegerGreaterThan as i128),
                Value::Integer(5),
            ]),
        ])];
        let mut context = ReportContext::from_stage(Stage::Retrieval(RetrievalMetadata::default()));
        filter(&input, &script, &mut context).unwrap();

        if let Stage::Retrieval(metadata) = context.stage {
            let expected_partial_results = vec![vec![
                vec![RadonInteger::from(2).into(), RadonInteger::from(3).into()],
                vec![RadonInteger::from(4).into(), RadonInteger::from(6).into()],
                vec![
                    RadonBoolean::from(false).into(),
                    RadonBoolean::from(true).into(),
                ],
            ]];
            assert_eq!(metadata.subscript_partial_results, expected_partial_results);
        }
    }

    #[test]
    fn test_filter_integer_greater_than() {
        let input = RadonArray::from(vec![
            RadonInteger::from(2).into(),
            RadonInteger::from(6).into(),
        ]);
        let script = vec![Value::Array(vec![Value::Array(vec![
            Value::Integer(IntegerGreaterThan as i128),
            Value::Integer(4),
        ])])];
        let output = filter(&input, &script, &mut ReportContext::default()).unwrap();

        let expected = RadonTypes::Array(RadonArray::from(vec![RadonInteger::from(6).into()]));

        assert_eq!(output, expected)
    }

    #[test]
    fn test_filter_negative() {
        let input = RadonArray::from(vec![
            RadonInteger::from(2).into(),
            RadonInteger::from(6).into(),
        ]);
        let script = vec![Value::Array(vec![Value::Array(vec![
            Value::Integer(IntegerMultiply as i128),
            Value::Integer(4),
        ])])];
        let result = filter(&input, &script, &mut ReportContext::default());

        assert_eq!(
            &result.unwrap_err().to_string(),
            "ArrayFilter subscript output was not RadonBoolean (was `RadonTypes::RadonInteger(8)`)"
        );
    }

    #[test]
    fn test_filter_operator() {
        let input = RadonArray::from(vec![
            RadonFloat::from(2.0).into(),
            RadonFloat::from(2.0).into(),
            RadonFloat::from(9.0).into(),
        ]);
        let filter_op = vec![
            Value::Integer(RadonFilters::DeviationStandard as i128),
            Value::Float(1.3),
        ];
        let output = filter(&input, &filter_op, &mut ReportContext::default()).unwrap();

        let expected = RadonTypes::Array(RadonArray::from(vec![
            RadonFloat::from(2.0).into(),
            RadonFloat::from(2.0).into(),
        ]));

        assert_eq!(output, expected)
    }

    #[test]
    fn test_sort_map_string_values() {
        let mut map1 = BTreeMap::new();
        map1.insert(
            "key1".to_string(),
            RadonTypes::String(RadonString::from("value1")),
        );
        map1.insert(
            "key2".to_string(),
            RadonTypes::String(RadonString::from("B")),
        );

        let mut map2 = BTreeMap::new();

        map2.insert(
            "key1".to_string(),
            RadonTypes::String(RadonString::from("value1")),
        );
        map2.insert(
            "key2".to_string(),
            RadonTypes::String(RadonString::from("A")),
        );

        let mut map3 = BTreeMap::new();

        map3.insert(
            "key1".to_string(),
            RadonTypes::String(RadonString::from("value1")),
        );
        map3.insert(
            "key2".to_string(),
            RadonTypes::String(RadonString::from("C")),
        );

        let input = RadonArray::from(vec![
            RadonMap::from(map1.clone()).into(),
            RadonMap::from(map2.clone()).into(),
            RadonMap::from(map3.clone()).into(),
        ]);
        let script = vec![Value::Array(vec![Value::Array(vec![
            Value::Integer(MapGetString as i128),
            Value::Text("key2".to_string()),
        ])])];
        let output = sort(&input, &script, &mut ReportContext::default()).unwrap();

        let expected = RadonTypes::from(RadonArray::from(vec![
            RadonMap::from(map2).into(),
            RadonMap::from(map1).into(),
            RadonMap::from(map3).into(),
        ]));

        assert_eq!(output, expected)
    }

    #[test]
    fn test_sort_map_integer_values() {
        let mut map1 = BTreeMap::new();
        map1.insert(
            "key1".to_string(),
            RadonTypes::Integer(RadonInteger::from(0)),
        );
        map1.insert(
            "key2".to_string(),
            RadonTypes::Integer(RadonInteger::from(1)),
        );

        let mut map2 = BTreeMap::new();

        map2.insert(
            "key1".to_string(),
            RadonTypes::Integer(RadonInteger::from(0)),
        );
        map2.insert(
            "key2".to_string(),
            RadonTypes::Integer(RadonInteger::from(2)),
        );

        let mut map3 = BTreeMap::new();

        map3.insert(
            "key1".to_string(),
            RadonTypes::Integer(RadonInteger::from(0)),
        );
        map3.insert(
            "key2".to_string(),
            RadonTypes::Integer(RadonInteger::from(-6)),
        );

        let input = RadonArray::from(vec![
            RadonMap::from(map1.clone()).into(),
            RadonMap::from(map2.clone()).into(),
            RadonMap::from(map3.clone()).into(),
        ]);
        let script = vec![Value::Array(vec![Value::Array(vec![
            Value::Integer(MapGetInteger as i128),
            Value::Text("key2".to_string()),
        ])])];
        let output = sort(&input, &script, &mut ReportContext::default()).unwrap();

        let expected = RadonTypes::from(RadonArray::from(vec![
            RadonMap::from(map3).into(),
            RadonMap::from(map1).into(),
            RadonMap::from(map2).into(),
        ]));

        assert_eq!(output, expected)
    }

    #[test]
    fn test_sort_identical_maps_integer_values() {
        let mut map1 = BTreeMap::new();
        map1.insert(
            "key1".to_string(),
            RadonTypes::Integer(RadonInteger::from(1)),
        );
        map1.insert(
            "key2".to_string(),
            RadonTypes::Integer(RadonInteger::from(1)),
        );

        let mut map2 = BTreeMap::new();

        map2.insert(
            "key1".to_string(),
            RadonTypes::Integer(RadonInteger::from(2)),
        );
        map2.insert(
            "key2".to_string(),
            RadonTypes::Integer(RadonInteger::from(1)),
        );

        let mut map3 = BTreeMap::new();

        map3.insert(
            "key1".to_string(),
            RadonTypes::Integer(RadonInteger::from(3)),
        );
        map3.insert(
            "key2".to_string(),
            RadonTypes::Integer(RadonInteger::from(1)),
        );

        let input = RadonArray::from(vec![
            RadonMap::from(map1.clone()).into(),
            RadonMap::from(map2.clone()).into(),
            RadonMap::from(map3.clone()).into(),
        ]);
        let script = vec![Value::Array(vec![Value::Array(vec![
            Value::Integer(MapGetInteger as i128),
            Value::Text("key2".to_string()),
        ])])];
        let output = sort(&input, &script, &mut ReportContext::default()).unwrap();

        let expected = RadonTypes::from(RadonArray::from(vec![
            RadonMap::from(map1).into(),
            RadonMap::from(map2).into(),
            RadonMap::from(map3).into(),
        ]));

        assert_eq!(output, expected)
    }

    #[test]
    fn test_sort_empty_map() {
        let map1 = BTreeMap::new();
        let map2 = BTreeMap::new();
        let map3 = BTreeMap::new();

        let input = RadonArray::from(vec![
            RadonMap::from(map1).into(),
            RadonMap::from(map2).into(),
            RadonMap::from(map3).into(),
        ]);
        let script = vec![Value::Array(vec![Value::Array(vec![
            Value::Integer(MapGetInteger as i128),
            Value::Text("key2".to_string()),
        ])])];
        let output = sort(&input, &script, &mut ReportContext::default()).unwrap_err();

        assert_eq!(output.to_string(), "Failed to get key `key2` from RadonMap")
    }

    #[test]
    fn test_sort_map_wrong_decode() {
        let item0 = RadonTypes::Integer(RadonInteger::from(0));
        let item1 = RadonTypes::Integer(RadonInteger::from(1));
        let mut map1 = BTreeMap::new();
        map1.insert("key1".to_string(), item0);
        map1.insert("key2".to_string(), item1);

        let input = RadonArray::from(vec![RadonMap::from(map1).into()]);
        let script = vec![Value::Array(vec![Value::Array(vec![
            Value::Integer(MapGetBoolean as i128),
            Value::Text("key2".to_string()),
        ])])];
        let output = sort(&input, &script, &mut ReportContext::default()).unwrap_err();

        if let RadError::UnhandledIntercept { inner, .. } = output {
            assert_eq!(
                inner.unwrap().to_string(),
                "Failed to decode RadonBoolean from RadonInteger"
            )
        } else {
            panic!();
        }
    }

    #[test]
    fn test_sort_map_floats_value() {
        let mut map1 = BTreeMap::new();
        map1.insert(
            "key1".to_string(),
            RadonTypes::Float(RadonFloat::from(std::f64::consts::PI)),
        );
        map1.insert(
            "key2".to_string(),
            RadonTypes::Float(RadonFloat::from(std::f64::consts::PI)),
        );

        let input = RadonArray::from(vec![RadonMap::from(map1).into()]);
        let script = vec![Value::Array(vec![Value::Array(vec![
            Value::Integer(MapGetFloat as i128),
            Value::Text("key2".to_string()),
        ])])];
        let output = sort(&input, &script, &mut ReportContext::default()).unwrap_err();
        let expected_err = RadError::UnsupportedSortOp { array: input };

        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_sort_string_2_arrays() {
        let input = RadonArray::from(vec![
            RadonString::from("Hello world!").into(),
            RadonString::from("Bye world!").into(),
        ]);
        let expected = RadonTypes::from(RadonArray::from(vec![
            RadonString::from("Bye world!").into(),
            RadonString::from("Hello world!").into(),
        ]));
        let output = sort(&input, &[], &mut ReportContext::default()).unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_sort_string_5_arrays() {
        let input = RadonArray::from(vec![
            RadonString::from("aa").into(),
            RadonString::from("ba").into(),
            RadonString::from("ab").into(),
            RadonString::from("a").into(),
            RadonString::from("").into(),
        ]);
        let expected = RadonTypes::from(RadonArray::from(vec![
            RadonString::from("").into(),
            RadonString::from("a").into(),
            RadonString::from("aa").into(),
            RadonString::from("ab").into(),
            RadonString::from("ba").into(),
        ]));
        let output = sort(&input, &[], &mut ReportContext::default()).unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_sort_string_4_arrays() {
        let input = RadonArray::from(vec![
            RadonString::from("a").into(),
            RadonString::from("Á").into(),
            RadonString::from("á").into(),
            RadonString::from("A").into(),
        ]);
        let expected = RadonTypes::from(RadonArray::from(vec![
            RadonString::from("A").into(),
            RadonString::from("a").into(),
            RadonString::from("Á").into(),
            RadonString::from("á").into(),
        ]));
        let output = sort(&input, &[], &mut ReportContext::default()).unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_sort_int_arrays() {
        let input = RadonArray::from(vec![
            RadonInteger::from(2i128).into(),
            RadonInteger::from(1i128).into(),
            RadonInteger::from(-2i128).into(),
            RadonInteger::from(0i128).into(),
        ]);
        let expected = RadonTypes::from(RadonArray::from(vec![
            RadonInteger::from(-2i128).into(),
            RadonInteger::from(0i128).into(),
            RadonInteger::from(1i128).into(),
            RadonInteger::from(2i128).into(),
        ]));
        let output = sort(&input, &[], &mut ReportContext::default()).unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_sort_float_arrays() {
        let input = RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]);
        let output = sort(&input, &[], &mut ReportContext::default()).unwrap_err();
        let expected_err = RadError::UnsupportedSortOp { array: input };

        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_sort_non_homogeneous_array() {
        let input = RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonInteger::from(2i128).into(),
        ]);
        let output = sort(&input, &[], &mut ReportContext::default()).unwrap_err();
        let expected_err = RadError::UnsupportedOpNonHomogeneous {
            operator: "ArraySort".to_string(),
        };

        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_sort_empty_array() {
        let input = RadonArray::from(vec![]);
        let expected = RadonTypes::from(RadonArray::from(vec![]));
        let output = sort(&input, &[], &mut ReportContext::default()).unwrap();
        assert_eq!(output, expected);
    }

    // Auxiliary functions
    fn radon_array_of_arrays() -> (RadonArray, i128, RadonArray) {
        let item0 = RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]);
        let item1 = RadonArray::from(vec![
            RadonFloat::from(11f64).into(),
            RadonFloat::from(12f64).into(),
        ]);
        let item2 = RadonArray::from(vec![
            RadonFloat::from(21f64).into(),
            RadonFloat::from(22f64).into(),
        ]);

        let output = RadonArray::from(vec![
            RadonTypes::from(item0),
            RadonTypes::from(item1.clone()),
            RadonTypes::from(item2),
        ]);

        (output, 1, item1)
    }

    fn radon_array_of_booleans() -> (RadonArray, i128, RadonBoolean) {
        let item0 = RadonBoolean::from(false);
        let item1 = RadonBoolean::from(true);
        let item2 = RadonBoolean::from(false);

        let output = RadonArray::from(vec![
            RadonTypes::from(item0),
            RadonTypes::from(item1.clone()),
            RadonTypes::from(item2),
        ]);

        (output, 1, item1)
    }

    fn radon_array_of_bytes() -> (RadonArray, i128, RadonBytes) {
        let item0 = RadonBytes::from(vec![0x01, 0x02, 0x03]);
        let item1 = RadonBytes::from(vec![0x11, 0x12, 0x13]);
        let item2 = RadonBytes::from(vec![0x21, 0x22, 0x23]);

        let output = RadonArray::from(vec![
            RadonTypes::from(item0),
            RadonTypes::from(item1.clone()),
            RadonTypes::from(item2),
        ]);

        (output, 1, item1)
    }

    fn radon_array_of_integers() -> (RadonArray, i128, RadonInteger) {
        let item0 = RadonInteger::from(1);
        let item1 = RadonInteger::from(11);
        let item2 = RadonInteger::from(21);

        let output = RadonArray::from(vec![
            RadonTypes::from(item0),
            RadonTypes::from(item1.clone()),
            RadonTypes::from(item2),
        ]);

        (output, 1, item1)
    }

    fn radon_array_of_floats() -> (RadonArray, i128, RadonFloat) {
        let item0 = RadonFloat::from(1.1f64);
        let item1 = RadonFloat::from(11.2f64);
        let item2 = RadonFloat::from(21.3f64);

        let output = RadonArray::from(vec![
            RadonTypes::from(item0),
            RadonTypes::from(item1.clone()),
            RadonTypes::from(item2),
        ]);

        (output, 1, item1)
    }

    fn radon_array_of_maps() -> (RadonArray, i128, RadonMap) {
        let mut map0 = BTreeMap::new();
        map0.insert(
            "key01".to_string(),
            RadonTypes::Integer(RadonInteger::from(1)),
        );
        map0.insert(
            "key02".to_string(),
            RadonTypes::Integer(RadonInteger::from(2)),
        );
        let item0 = RadonMap::from(map0);

        let mut map1 = BTreeMap::new();
        map1.insert(
            "key11".to_string(),
            RadonTypes::Integer(RadonInteger::from(11)),
        );
        map1.insert(
            "key12".to_string(),
            RadonTypes::Integer(RadonInteger::from(12)),
        );
        let item1 = RadonMap::from(map1);

        let mut map2 = BTreeMap::new();
        map2.insert(
            "key21".to_string(),
            RadonTypes::Integer(RadonInteger::from(21)),
        );
        map2.insert(
            "key22".to_string(),
            RadonTypes::Integer(RadonInteger::from(22)),
        );
        let item2 = RadonMap::from(map2);

        let output = RadonArray::from(vec![
            RadonTypes::from(item0),
            RadonTypes::from(item1.clone()),
            RadonTypes::from(item2),
        ]);

        (output, 1, item1)
    }

    fn radon_array_of_strings() -> (RadonArray, i128, RadonString) {
        let item0 = RadonString::from("Hello");
        let item1 = RadonString::from("World");
        let item2 = RadonString::from("Rust");

        let output = RadonArray::from(vec![
            RadonTypes::from(item0),
            RadonTypes::from(item1.clone()),
            RadonTypes::from(item2),
        ]);

        (output, 1, item1)
    }

    #[test]
    fn test_get_array() {
        let (input, index, item) = radon_array_of_arrays();
        let output = get::<RadonArray, _>(&input, &[Value::Integer(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_get_array_fail() {
        let (input, index, _item) = radon_array_of_floats();
        let output = get::<RadonArray, _>(&input, &[Value::Integer(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: RadonFloat::radon_type_name(),
            to: RadonArray::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_get_boolean() {
        let (input, index, item) = radon_array_of_booleans();
        let output = get::<RadonBoolean, _>(&input, &[Value::Integer(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_get_boolean_fail() {
        let (input, index, _item) = radon_array_of_floats();
        let output = get::<RadonBoolean, _>(&input, &[Value::Integer(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: RadonFloat::radon_type_name(),
            to: RadonBoolean::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_get_bytes() {
        let (input, index, item) = radon_array_of_bytes();
        let output = get::<RadonBytes, _>(&input, &[Value::Integer(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_get_bytes_fail() {
        let (input, index, _item) = radon_array_of_floats();
        let output = get::<RadonBytes, _>(&input, &[Value::Integer(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: RadonFloat::radon_type_name(),
            to: RadonBytes::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_get_integer() {
        let (input, index, item) = radon_array_of_integers();
        let output = get_number::<RadonInteger>(&input, &[Value::Integer(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_get_integer_fail() {
        let (input, index, _item) = radon_array_of_floats();
        let output = get_number::<RadonInteger>(&input, &[Value::Integer(index)]).unwrap_err();
        let expected_err = RadError::ParseInt {
            message: "invalid digit found in string".to_string(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_get_float() {
        let (input, index, item) = radon_array_of_floats();
        let output = get_number::<RadonFloat>(&input, &[Value::Integer(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_get_float_fail() {
        let (input, index, _item) = radon_array_of_arrays();
        let output = get_number::<RadonFloat>(&input, &[Value::Integer(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: RadonArray::radon_type_name(),
            to: RadonString::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_get_map() {
        let (input, index, item) = radon_array_of_maps();
        let output = get::<RadonMap, _>(&input, &[Value::Integer(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_get_map_fail() {
        let (input, index, _item) = radon_array_of_floats();
        let output = get::<RadonMap, _>(&input, &[Value::Integer(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: RadonFloat::radon_type_name(),
            to: RadonMap::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_get_string() {
        let (input, index, item) = radon_array_of_strings();
        let output = get::<RadonString, _>(&input, &[Value::Integer(index)]).unwrap();
        assert_eq!(output, item);
    }

    #[test]
    fn test_get_string_fail() {
        let (input, index, _item) = radon_array_of_arrays();
        let output = get::<RadonString, _>(&input, &[Value::Integer(index)]).unwrap_err();
        let expected_err = RadError::Decode {
            from: "RadonArray",
            to: RadonString::radon_type_name(),
        };
        assert_eq!(output, expected_err);
    }

    #[test]
    fn test_get_string_from_integer_wont_fail() {
        let (input, index, _item) = radon_array_of_integers();
        let output = get::<RadonString, _>(&input, &[Value::Integer(index)]).unwrap();
        let expected = RadonString::from("11");
        assert_eq!(output, expected);
    }

    #[test]
    fn test_get_string_from_float_wont_fail() {
        let (input, index, _item) = radon_array_of_floats();
        let output = get::<RadonString, _>(&input, &[Value::Integer(index)]).unwrap();
        let expected = RadonString::from("11.2");
        assert_eq!(output, expected);
    }

    #[test]
    fn test_get_float_with_separators() {
        let expected = RadonTypes::from(RadonFloat::from(1234.567));

        // English style numbers, i.e. commas for thousands and dots for decimals.
        let array = RadonArray::from(vec![RadonTypes::from(RadonString::from("1,234.567"))]);
        let output = array
            .operate(&(
                RadonOpCodes::ArrayGetFloat,
                Some(vec![serde_cbor::Value::from(0)]),
            ))
            .unwrap();
        assert_eq!(output, expected);

        // Spanish/Italian/German/Norwegian style, i.e. dots for thousands, commas for decimals
        let array = RadonArray::from(vec![RadonTypes::from(RadonString::from("1234,567"))]);
        let output = array
            .operate(&(
                RadonOpCodes::ArrayGetFloat,
                Some(vec![
                    serde_cbor::Value::from(0),
                    serde_cbor::Value::from(String::from(".")),
                    serde_cbor::Value::from(String::from(",")),
                ]),
            ))
            .unwrap();
        assert_eq!(output, expected);

        // Spanish/Italian/German/Norwegian style, i.e. dots for thousands, commas for decimals
        let array = RadonArray::from(vec![RadonTypes::from(RadonString::from("1.234,567"))]);
        let output = array
            .operate(&(
                RadonOpCodes::ArrayGetFloat,
                Some(vec![
                    serde_cbor::Value::from(0),
                    serde_cbor::Value::from(String::from(".")),
                    serde_cbor::Value::from(String::from(",")),
                ]),
            ))
            .unwrap();
        assert_eq!(output, expected);

        // Danish/Finnish/French/Canadian/Swedish style, i.e. spaces for thousands, commas for decimals
        let array = RadonArray::from(vec![RadonTypes::from(RadonString::from("1 234,567"))]);
        let output = array
            .operate(&(
                RadonOpCodes::ArrayGetFloat,
                Some(vec![
                    serde_cbor::Value::from(0),
                    serde_cbor::Value::from(String::from(" ")),
                    serde_cbor::Value::from(String::from(",")),
                ]),
            ))
            .unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_get_integer_with_separators() {
        let expected = RadonTypes::from(RadonInteger::from(1234));

        // English style numbers, i.e. commas for thousands and dots for decimals.
        let array = RadonArray::from(vec![RadonTypes::from(RadonString::from("1,234"))]);
        let output = array
            .operate(&(
                RadonOpCodes::ArrayGetInteger,
                Some(vec![serde_cbor::Value::from(0)]),
            ))
            .unwrap();
        assert_eq!(output, expected);

        // Spanish/Italian/German/Norwegian style, i.e. dots for thousands, commas for decimals
        let array = RadonArray::from(vec![RadonTypes::from(RadonString::from("1.234"))]);
        let output = array
            .operate(&(
                RadonOpCodes::ArrayGetInteger,
                Some(vec![
                    serde_cbor::Value::from(0),
                    serde_cbor::Value::from(String::from(".")),
                ]),
            ))
            .unwrap();
        assert_eq!(output, expected);

        // Danish/Finnish/French/Canadian/Swedish style, i.e. spaces for thousands, commas for decimals
        let array = RadonArray::from(vec![RadonTypes::from(RadonString::from("1 234"))]);
        let output = array
            .operate(&(
                RadonOpCodes::ArrayGetInteger,
                Some(vec![
                    serde_cbor::Value::from(0),
                    serde_cbor::Value::from(String::from(" ")),
                ]),
            ))
            .unwrap();
        assert_eq!(output, expected);
    }
}
