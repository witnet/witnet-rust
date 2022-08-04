use std::convert::TryFrom;

use serde_cbor::{
    self as cbor,
    value::{from_value, Value},
};
use witnet_data_structures::{
    chain::{tapi::ActiveWips, RADFilter},
    radon_report::{RadonReport, ReportContext, Stage},
};

use crate::{
    error::RadError,
    filters::RadonFilters,
    operators::{operate, operate_in_context, RadonOpCodes},
    reducers::RadonReducers,
    types::RadonTypes,
};

pub type RadonCall = (RadonOpCodes, Option<Vec<Value>>);
pub type RadonScript = Vec<RadonCall>;

/// A set of flags for telling the RADON executor what execution features to enable and what
/// metadata to collect.
#[derive(Clone, Copy)]
pub struct RadonScriptExecutionSettings {
    /// Keep track of which call index is being executed in every moment, so that any error can be
    /// pinpointed to a specific call in the script.
    pub breakpoints: bool,
    /// Keep the result of each of the calls in the script, so that the full execution trace can be
    /// reconstructed.
    pub partial_results: bool,
    /// Measure total execution time for the script.
    pub timing: bool,
}

/// Default to enabling all execution features except `partial_results`.
impl Default for RadonScriptExecutionSettings {
    fn default() -> Self {
        Self::all_but_partial_results()
    }
}

impl RadonScriptExecutionSettings {
    /// Enable all execution features except `partial_results`. This is the default for
    /// `witnet_node`.
    pub fn all_but_partial_results() -> Self {
        Self {
            partial_results: false,
            ..Self::enable_all()
        }
    }

    /// Disable all execution features.
    pub fn disable_all() -> Self {
        Self {
            partial_results: false,
            timing: false,
            breakpoints: false,
        }
    }

    /// Enable all execution features. This is recommended for `witnet_wallet`.
    pub fn enable_all() -> Self {
        Self {
            partial_results: true,
            timing: true,
            breakpoints: true,
        }
    }

    /// Only enable the execution features that are suitable for a specific data request stage.
    pub fn tailored_to_stage(stage: &Stage<RadonTypes>) -> Self {
        match stage {
            Stage::Retrieval(_) => Self::enable_all(),
            _ => Self::all_but_partial_results(),
        }
    }
}

/// Run any RADON script on given input data, and return `RadonReport`.
/// By enabling or disabling each of the specific flags in the settings argument, we can adjust how
/// much execution metadata we want to track, e.g. execution time, partial results, etc.
pub fn execute_radon_script(
    input: RadonTypes,
    script: &[RadonCall],
    context: &mut ReportContext<RadonTypes>,
    settings: RadonScriptExecutionSettings,
) -> Result<RadonReport<RadonTypes>, RadError> {
    // Set the execution start timestamp, if enabled by `timing` setting
    if settings.timing {
        context.start();
    }

    // Initialize a vector for storing the partial results, if enabled by `partial_results` setting
    let mut partial_results = if settings.partial_results {
        Some(vec![Ok(input.clone())])
    } else {
        None
    };

    // Run the execution by recursively applying calls into the result of the previous call
    let result = script
        .iter()
        .enumerate()
        .try_fold(input, |input, (i, call)| {
            // Update the index of the last executed call, if enabled in settings
            if settings.breakpoints {
                context.call_index = Some(i);
            }

            // Apply the call
            let partial_result = operate_in_context(input, call, context);

            // Keep partial result, if enabled by `partial_results` setting
            if let Some(partial_results) = partial_results.as_mut() {
                partial_results.push(partial_result.clone());
            }

            partial_result
        });

    // Set the completion timestamp, if enabled by `timing` settings
    if settings.timing {
        context.complete();
    }

    // Return a report as constructed from the result and the context
    Ok(if let Some(partial_results) = partial_results {
        RadonReport::from_partial_results(partial_results, context)
    } else {
        RadonReport::from_result(result, context)
    })
}

/// Run any RADON script on given input data, and return `RadonTypes`.
/// This the optimistic version of `execute_radon_script`, as it returns a value or an error, but
/// gives no specific details on what happened during the execution
pub fn execute_contextfree_radon_script(
    input: RadonTypes,
    script: &[RadonCall],
) -> Result<RadonTypes, RadError> {
    script.iter().try_fold(input, operate)
}

pub fn unpack_radon_script(packed: &[u8]) -> Result<RadonScript, RadError> {
    match cbor::from_slice(packed) {
        Ok(Value::Array(array)) => array
            .iter()
            .map(unpack_radon_call)
            .collect::<Result<RadonScript, RadError>>(),
        Ok(_) => Err(errorify(RadError::ScriptNotArray {
            input_type: String::from("different thing"),
        })),
        Err(error) => Err(errorify(RadError::BufferIsNotValue {
            description: error.to_string(),
        })),
    }
}

pub fn unpack_radon_call(packed_call: &Value) -> Result<RadonCall, RadError> {
    match packed_call {
        Value::Array(array) => unpack_compound_call(array),
        Value::Integer(integer) => {
            if *integer >= 0i128 {
                let [raw_op_code, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _] =
                    integer.to_le_bytes();
                RadonOpCodes::try_from(raw_op_code)
                    .map(|op_code| (op_code, None))
                    .map_err(|_| errorify(RadError::UnknownOperator { code: *integer }))
            } else {
                Err(errorify(RadError::NotNaturalOperator { code: *integer }))
            }
        }
        _ => Err(errorify(RadError::NotIntegerOperator)),
    }
}

fn unpack_compound_call(array: &[Value]) -> Result<RadonCall, RadError> {
    let (head, tail) = array
        .split_first()
        .ok_or_else(|| errorify(RadError::NoOperatorInCompoundCall))?;
    let op_code =
        from_value::<u8>(head.to_owned()).map_err(|_| errorify(RadError::NotIntegerOperator))?;
    let op_code =
        RadonOpCodes::try_from(op_code).map_err(|_| errorify(RadError::NotIntegerOperator))?;

    Ok((op_code, Some(tail.to_vec())))
}

pub fn unpack_subscript(value: &Value) -> Result<Vec<RadonCall>, RadError> {
    let mut subscript = vec![];
    let subscript_arg = match value {
        Value::Array(x) => x,
        x => return Err(RadError::BadSubscriptFormat { value: x.clone() }),
    };
    for arg in subscript_arg {
        subscript.push(unpack_radon_call(arg)?)
    }

    Ok(subscript)
}

fn errorify(kind: RadError) -> RadError {
    log::error!("Error unpacking a RADON script: {:?}", kind);

    kind
}

pub fn create_radon_script_from_filters_and_reducer(
    filters: &[RADFilter],
    reducer: u32,
    active_wips: &ActiveWips,
) -> Result<Vec<RadonCall>, RadError> {
    let unknown_filter = |code| RadError::UnknownFilter { code };
    let unknown_reducer = |code| RadError::UnknownReducer { code };

    let mut radoncall_vec = vec![];
    for filter in filters {
        let filter_op = i128::from(filter.op);
        let rad_filter =
            RadonFilters::try_from(u8::try_from(filter_op).map_err(|_| unknown_filter(filter_op))?)
                .map_err(|_| unknown_filter(filter_op))?;

        // TODO: Update with more filters
        match rad_filter {
            RadonFilters::DeviationStandard | RadonFilters::Mode => {}
            _ => {
                return Err(RadError::UnsupportedFilterInAT {
                    operator: rad_filter as u8,
                })
            }
        };

        let args = if filter.args.is_empty() {
            Some(vec![Value::Integer(filter_op)])
        } else {
            let filter_args = cbor::from_slice(filter.args.as_slice()).map_err(|e| {
                errorify(RadError::BufferIsNotValue {
                    description: e.to_string(),
                })
            })?;

            Some(vec![Value::Integer(filter_op), filter_args])
        };

        radoncall_vec.push((RadonOpCodes::ArrayFilter, args));
    }

    let rad_reducer = RadonReducers::try_from(
        u8::try_from(reducer).map_err(|_| unknown_reducer(i128::from(reducer)))?,
    )
    .map_err(|_| unknown_reducer(i128::from(reducer)))?;
    match rad_reducer {
        RadonReducers::AverageMean | RadonReducers::Mode => {}
        RadonReducers::AverageMedian => {
            if !active_wips.wip0017() {
                return Err(RadError::UnsupportedReducerInAT {
                    operator: rad_reducer as u8,
                });
            }
        }
        RadonReducers::HashConcatenate => {
            if !active_wips.wip0019() {
                return Err(RadError::UnsupportedReducerInAT {
                    operator: rad_reducer as u8,
                });
            }
        }
        _ => {
            return Err(RadError::UnsupportedReducerInAT {
                operator: rad_reducer as u8,
            })
        }
    };

    let args = Some(vec![Value::Integer(i128::from(reducer))]);
    radoncall_vec.push((RadonOpCodes::ArrayReduce, args));

    Ok(radoncall_vec)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::current_active_wips;

    use super::*;

    #[test]
    fn test_execute_radon_script() {
        use crate::types::{
            array::RadonArray, float::RadonFloat, integer::RadonInteger, map::RadonMap,
            string::RadonString,
        };

        let input = RadonTypes::from(RadonString::from(
            r#"{"coord":{"lon":13.41,"lat":52.52},"weather":[{"id":600,"main":"Snow","description":"light snow","icon":"13n"}],"base":"stations","main":{"temp":-4,"pressure":1013,"humidity":73,"temp_min":-4,"temp_max":-4},"visibility":10000,"wind":{"speed":2.6,"deg":90},"clouds":{"all":75},"dt":1548346800,"sys":{"type":1,"id":1275,"message":0.0038,"country":"DE","sunrise":1548313160,"sunset":1548344298},"id":2950159,"name":"Berlin","cod":200}"#,
        ));
        let script = vec![
            (RadonOpCodes::StringParseJSONMap, None),
            (
                RadonOpCodes::MapGetMap,
                Some(vec![Value::Text(String::from("main"))]),
            ),
            (
                RadonOpCodes::MapGetFloat,
                Some(vec![Value::Text(String::from("temp"))]),
            ),
        ];

        // Test context-free execution
        let output = execute_contextfree_radon_script(input.clone(), &script).unwrap();
        let expected = RadonTypes::from(RadonFloat::from(-4.0));
        assert_eq!(output, expected);

        // Test contextful execution
        let mut context = ReportContext::default();
        let output = execute_radon_script(
            input.clone(),
            &script,
            &mut context,
            RadonScriptExecutionSettings::enable_all(),
        )
        .unwrap();
        let partial_expected = vec![
            input,
            RadonTypes::from(RadonMap::from(
                vec![
                    (
                        String::from("base"),
                        RadonTypes::from(RadonString::from("stations")),
                    ),
                    (
                        String::from("clouds"),
                        RadonTypes::from(RadonMap::from(
                            vec![(
                                String::from("all"),
                                RadonTypes::from(RadonInteger::from(75)),
                            )]
                            .iter()
                            .cloned()
                            .collect::<BTreeMap<String, RadonTypes>>(),
                        )),
                    ),
                    (
                        String::from("cod"),
                        RadonTypes::from(RadonInteger::from(200)),
                    ),
                    (
                        String::from("coord"),
                        RadonTypes::from(RadonMap::from(
                            vec![
                                (
                                    String::from("lon"),
                                    RadonTypes::from(RadonFloat::from(13.41)),
                                ),
                                (
                                    String::from("lat"),
                                    RadonTypes::from(RadonFloat::from(52.52)),
                                ),
                            ]
                            .iter()
                            .cloned()
                            .collect::<BTreeMap<String, RadonTypes>>(),
                        )),
                    ),
                    (
                        String::from("dt"),
                        RadonTypes::from(RadonInteger::from(1_548_346_800)),
                    ),
                    (
                        String::from("id"),
                        RadonTypes::from(RadonInteger::from(2_950_159)),
                    ),
                    (
                        String::from("main"),
                        RadonTypes::from(RadonMap::from(
                            vec![
                                (
                                    String::from("temp"),
                                    RadonTypes::from(RadonInteger::from(-4)),
                                ),
                                (
                                    String::from("pressure"),
                                    RadonTypes::from(RadonInteger::from(1013)),
                                ),
                                (
                                    String::from("humidity"),
                                    RadonTypes::from(RadonInteger::from(73)),
                                ),
                                (
                                    String::from("temp_min"),
                                    RadonTypes::from(RadonInteger::from(-4)),
                                ),
                                (
                                    String::from("temp_max"),
                                    RadonTypes::from(RadonInteger::from(-4)),
                                ),
                            ]
                            .iter()
                            .cloned()
                            .collect::<BTreeMap<String, RadonTypes>>(),
                        )),
                    ),
                    (
                        String::from("name"),
                        RadonTypes::from(RadonString::from("Berlin")),
                    ),
                    (
                        String::from("sys"),
                        RadonTypes::from(RadonMap::from(
                            vec![
                                (
                                    String::from("type"),
                                    RadonTypes::from(RadonInteger::from(1)),
                                ),
                                (
                                    String::from("id"),
                                    RadonTypes::from(RadonInteger::from(1275)),
                                ),
                                (
                                    String::from("message"),
                                    RadonTypes::from(RadonFloat::from(0.0038)),
                                ),
                                (
                                    String::from("country"),
                                    RadonTypes::from(RadonString::from("DE")),
                                ),
                                (
                                    String::from("sunrise"),
                                    RadonTypes::from(RadonInteger::from(1_548_313_160)),
                                ),
                                (
                                    String::from("sunset"),
                                    RadonTypes::from(RadonInteger::from(1_548_344_298)),
                                ),
                            ]
                            .iter()
                            .cloned()
                            .collect::<BTreeMap<String, RadonTypes>>(),
                        )),
                    ),
                    (
                        String::from("visibility"),
                        RadonTypes::from(RadonInteger::from(10000)),
                    ),
                    (
                        String::from("weather"),
                        RadonTypes::from(RadonArray::from(vec![RadonTypes::from(RadonMap::from(
                            vec![
                                (
                                    String::from("id"),
                                    RadonTypes::from(RadonInteger::from(600)),
                                ),
                                (
                                    String::from("main"),
                                    RadonTypes::from(RadonString::from("Snow")),
                                ),
                                (
                                    String::from("description"),
                                    RadonTypes::from(RadonString::from("light snow")),
                                ),
                                (
                                    String::from("icon"),
                                    RadonTypes::from(RadonString::from("13n")),
                                ),
                            ]
                            .iter()
                            .cloned()
                            .collect::<BTreeMap<String, RadonTypes>>(),
                        ))])),
                    ),
                    (
                        String::from("wind"),
                        RadonTypes::from(RadonMap::from(
                            vec![
                                (
                                    String::from("speed"),
                                    RadonTypes::from(RadonFloat::from(2.6)),
                                ),
                                (
                                    String::from("deg"),
                                    RadonTypes::from(RadonInteger::from(90)),
                                ),
                            ]
                            .iter()
                            .cloned()
                            .collect::<BTreeMap<String, RadonTypes>>(),
                        )),
                    ),
                ]
                .iter()
                .cloned()
                .collect::<BTreeMap<String, RadonTypes>>(),
            )),
            RadonTypes::from(RadonMap::from(
                vec![
                    (
                        String::from("temp"),
                        RadonTypes::from(RadonInteger::from(-4)),
                    ),
                    (
                        String::from("pressure"),
                        RadonTypes::from(RadonInteger::from(1013)),
                    ),
                    (
                        String::from("humidity"),
                        RadonTypes::from(RadonInteger::from(73)),
                    ),
                    (
                        String::from("temp_min"),
                        RadonTypes::from(RadonInteger::from(-4)),
                    ),
                    (
                        String::from("temp_max"),
                        RadonTypes::from(RadonInteger::from(-4)),
                    ),
                ]
                .iter()
                .cloned()
                .collect::<BTreeMap<String, RadonTypes>>(),
            )),
            RadonTypes::from(RadonFloat::from(-4.0)),
        ];
        assert_eq!(output.result, expected);
        assert_eq!(output.partial_results, Some(partial_expected));
    }

    #[test]
    fn test_floats_as_integers() {
        use crate::types::{integer::RadonInteger, string::RadonString};

        let good_input = RadonTypes::from(RadonString::from(r#"{"data": 4.0}"#));
        let bad_input = RadonTypes::from(RadonString::from(r#"{"data": 4.1}"#));
        let script = vec![
            (RadonOpCodes::StringParseJSONMap, None),
            (
                RadonOpCodes::MapGetInteger,
                Some(vec![Value::Text(String::from("data"))]),
            ),
        ];

        let good_output = execute_contextfree_radon_script(good_input, &script).unwrap();
        let bad_output = execute_contextfree_radon_script(bad_input, &script).unwrap_err();

        assert_eq!(good_output, RadonTypes::from(RadonInteger::from(4)));
        assert_eq!(
            bad_output,
            RadError::ParseInt {
                message: "invalid digit found in string".to_string()
            }
        );
    }

    #[test]
    fn test_unpack_radon_script() {
        let cbor_vec = Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONMap as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text(String::from("main")),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetFloat as i128),
                Value::Text(String::from("temp")),
            ]),
        ]);
        let packed = serde_cbor::to_vec(&cbor_vec).unwrap();

        let expected = vec![
            (RadonOpCodes::StringParseJSONMap, None),
            (
                RadonOpCodes::MapGetMap,
                Some(vec![Value::Text(String::from("main"))]),
            ),
            (
                RadonOpCodes::MapGetFloat,
                Some(vec![Value::Text(String::from("temp"))]),
            ),
        ];
        println!("{:?}", expected);

        let output = unpack_radon_script(&packed).unwrap();

        assert_eq!(output, expected)
    }

    #[test]
    fn test_create_radon_script() {
        let expected = vec![
            (
                RadonOpCodes::ArrayFilter,
                Some(vec![
                    Value::Integer(RadonFilters::DeviationStandard as i128),
                    Value::Float(1.0),
                ]),
            ),
            (
                RadonOpCodes::ArrayReduce,
                Some(vec![Value::Integer(RadonReducers::AverageMean as i128)]),
            ),
        ];

        let filters = vec![RADFilter {
            op: RadonFilters::DeviationStandard as u32,
            args: vec![249, 60, 0],
        }];
        let reducer = RadonReducers::AverageMean as u32;
        let output = create_radon_script_from_filters_and_reducer(
            filters.as_slice(),
            reducer,
            &current_active_wips(),
        )
        .unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_create_radon_script_invalid_filter() {
        let filters = vec![RADFilter {
            op: RadonFilters::DeviationAbsolute as u32,
            args: vec![249, 60, 0],
        }];
        let reducer = RadonReducers::AverageMean as u32;
        let output = create_radon_script_from_filters_and_reducer(
            filters.as_slice(),
            reducer,
            &current_active_wips(),
        )
        .unwrap_err();

        let expected = RadError::UnsupportedFilterInAT {
            operator: RadonFilters::DeviationAbsolute as u8,
        };
        assert_eq!(output, expected);

        let filters = vec![RADFilter {
            op: 99,
            args: vec![],
        }];
        let output = create_radon_script_from_filters_and_reducer(
            filters.as_slice(),
            reducer,
            &current_active_wips(),
        )
        .unwrap_err();

        let expected = RadError::UnknownFilter { code: 99 };
        assert_eq!(output, expected);
    }

    #[test]
    fn test_create_radon_script_invalid_reducer() {
        let filters = vec![RADFilter {
            op: RadonFilters::DeviationStandard as u32,
            args: vec![249, 60, 0],
        }];
        let reducer = RadonReducers::Min as u32;
        let output = create_radon_script_from_filters_and_reducer(
            filters.as_slice(),
            reducer,
            &current_active_wips(),
        )
        .unwrap_err();

        let expected = RadError::UnsupportedReducerInAT {
            operator: RadonReducers::Min as u8,
        };
        assert_eq!(output, expected);

        let output = create_radon_script_from_filters_and_reducer(
            filters.as_slice(),
            99,
            &current_active_wips(),
        )
        .unwrap_err();

        let expected = RadError::UnknownReducer { code: 99 };
        assert_eq!(output, expected);
    }
}
