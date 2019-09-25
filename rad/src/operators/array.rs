use crate::error::RadError;
use crate::reducers::{self, RadonReducers};
use crate::script::{execute_radon_script, unpack_radon_call};
use crate::types::{array::RadonArray, integer::RadonInteger, RadonType, RadonTypes};

use num_traits::FromPrimitive;
use serde_cbor::value::{from_value, Value};
use std::clone::Clone;

pub fn count(input: &RadonArray) -> RadonInteger {
    RadonInteger::from(input.value().len() as i128)
}

pub fn reduce(input: &RadonArray, args: &[Value]) -> Result<RadonTypes, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: "RadonArray".to_string(),
        operator: "Reduce".to_string(),
        args: args.to_vec(),
    };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let reducer_integer = from_value::<i64>(arg).map_err(|_| wrong_args())?;
    let reducer_code = RadonReducers::from_i64(reducer_integer).ok_or_else(wrong_args)?;

    reducers::reduce(input, reducer_code)
}

pub fn get(input: &RadonArray, args: &[Value]) -> Result<RadonTypes, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: "RadonArray".to_string(),
        operator: "Reduce".to_string(),
        args: args.to_vec(),
    };

    let not_found = |index: i32| RadError::ArrayIndexNotFound { index };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let index = from_value::<i32>(arg).map_err(|_| wrong_args())?;

    input
        .value()
        .get(index as usize)
        .map(Clone::clone)
        .ok_or_else(|| not_found(index))
}

pub fn map(input: &RadonArray, args: &[Value]) -> Result<RadonTypes, RadError> {
    let mut subscript = vec![];
    for arg in args {
        subscript.push(unpack_radon_call(arg)?)
    }

    let mut result = vec![];
    for item in input.value() {
        result.push(execute_radon_script(item, subscript.as_slice())?);
    }

    Ok(RadonArray::from(result).into())
}

pub fn filter(input: &RadonArray, args: &[Value]) -> Result<RadonTypes, RadError> {
    let mut subscript = vec![];
    for arg in args {
        subscript.push(unpack_radon_call(arg)?)
    }

    let mut result = vec![];
    for item in input.value() {
        match execute_radon_script(item.clone(), subscript.as_slice())? {
            RadonTypes::Boolean(boolean) => {
                if boolean.value() {
                    result.push(item);
                }
            }
            value => Err(RadError::ArrayFilterWrongSubscript {
                value: value.to_string(),
            })?,
        }
    }

    Ok(RadonArray::from(result).into())
}

pub fn sort(input: &RadonArray, args: &[Value]) -> Result<RadonArray, RadError> {
    let input_value = input.value();
    let mapped_array = map(input, args)?;
    let mapped_array = match mapped_array {
        RadonTypes::Array(x) => x,
        _ => unreachable!(),
    };

    let mapped_array_value = mapped_array.value();
    let mut tuple_array: Vec<(&RadonTypes, &RadonTypes)> =
        input_value.iter().zip(mapped_array_value.iter()).collect();
    // if input is empty, return the array
    if input.value().is_empty() {
        return Ok(input.clone());
    }
    // Sort not applicable if not homogeneous
    if !input.is_homogeneous() {
        Err(RadError::UnsupportedOpNonHomogeneous {
            operator: "ArraySort".to_string(),
        })?;
    }

    // Distinguish depending the type
    match &mapped_array_value.first() {
        Some(RadonTypes::String(_)) => {
            tuple_array.sort_by(|a, b| match (a.1, b.1) {
                (RadonTypes::String(a), RadonTypes::String(b)) => a.cmp(&b),
                _ => unreachable!(),
            });
        }
        Some(RadonTypes::Integer(_)) => {
            tuple_array.sort_by(|a, b| match (a.1, b.1) {
                (RadonTypes::Integer(a), RadonTypes::Integer(b)) => a.cmp(&b),
                _ => unreachable!(),
            });
        }
        _ => Err(RadError::UnsupportedSortOp {
            inner_type: mapped_array_value[0].clone().radon_type_name(),
        })?,
    };

    let result: Vec<_> = tuple_array.into_iter().map(|(a, _)| a.clone()).collect();

    Ok(RadonArray::from(result))
}

pub fn transpose(input: &RadonArray) -> Result<Vec<RadonArray>, RadError> {
    let mut v = vec![];
    let mut prev_len = None;
    for item in input.value() {
        match item {
            RadonTypes::Array(rad_value) => {
                let sub_value = rad_value.value();
                let sub_value_len = sub_value.len();

                if prev_len.is_none() {
                    for sub_item in rad_value.value().into_iter() {
                        v.push(vec![sub_item]);
                    }
                    prev_len = Some(sub_value_len);
                } else if prev_len == Some(sub_value_len) {
                    for (i, sub_item) in rad_value.value().into_iter().enumerate() {
                        v[i].push(sub_item);
                    }
                } else {
                    Err(RadError::DifferentSizeArrays {
                        method: "RadonArray::transpose".to_string(),
                        first: prev_len.unwrap(),
                        second: sub_value_len,
                    })?
                }
            }
            _ => Err(RadError::MismatchingTypes {
                method: "RadonArray::transpose".to_string(),
                expected: RadonArray::radon_type_name(),
                found: item.radon_type_name(),
            })?,
        }
    }

    Ok(v.into_iter().map(RadonArray::from).collect())
}

#[test]
fn test_array_count() {
    use crate::types::float::RadonFloat;

    let input = &RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);

    let empty = &RadonArray::from(vec![]);

    assert_eq!(count(&input), RadonInteger::from(2));
    assert_eq!(count(&empty), RadonInteger::from(0));
}

#[test]
fn test_reduce_no_args() {
    use crate::types::float::RadonFloat;

    let input = &RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let args = &[];

    let result = reduce(input, args);

    assert_eq!(
        &result.unwrap_err().to_string(),
        "Wrong `RadonArray::Reduce()` arguments: `[]`"
    );
}

#[test]
fn test_reduce_wrong_args() {
    use crate::types::float::RadonFloat;

    let input = &RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let args = &[Value::Text(String::from("wrong"))]; // This is RadonReducers::AverageMean

    let result = reduce(input, args);

    assert_eq!(
        &result.unwrap_err().to_string(),
        "Wrong `RadonArray::Reduce()` arguments: `[Text(\"wrong\")]`"
    );
}

#[test]
fn test_reduce_unknown_reducer() {
    use crate::types::float::RadonFloat;

    let input = &RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let args = &[Value::Integer(-1)]; // This doesn't match any reducer code in RadonReducers

    let result = reduce(input, args);

    assert_eq!(
        &result.unwrap_err().to_string(),
        "Wrong `RadonArray::Reduce()` arguments: `[Integer(-1)]`"
    );
}

#[test]
fn test_transpose() {
    use crate::types::{float::RadonFloat, RadonTypes};

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

    let v1 = RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(11f64).into(),
    ]);
    let v2 = RadonArray::from(vec![
        RadonFloat::from(2f64).into(),
        RadonFloat::from(12f64).into(),
    ]);
    let v3 = RadonArray::from(vec![
        RadonFloat::from(3f64).into(),
        RadonFloat::from(13f64).into(),
    ]);
    let expected = vec![v1, v2, v3];

    let output = transpose(&input).unwrap();

    assert_eq!(output, expected);
}

#[test]
fn test_map_integer_greater_than() {
    use crate::operators::RadonOpCodes::IntegerGreaterThan;
    use crate::types::boolean::RadonBoolean;

    let input = RadonArray::from(vec![
        RadonInteger::from(2).into(),
        RadonInteger::from(6).into(),
    ]);
    let script = vec![Value::Array(vec![
        Value::Integer(IntegerGreaterThan as i128),
        Value::Integer(4),
    ])];
    let output = map(&input, &script).unwrap();

    let expected = RadonTypes::Array(RadonArray::from(vec![
        RadonBoolean::from(false).into(),
        RadonBoolean::from(true).into(),
    ]));

    assert_eq!(output, expected)
}

#[test]
fn test_filter_integer_greater_than() {
    use crate::operators::RadonOpCodes::IntegerGreaterThan;
    use crate::types::integer::RadonInteger;

    let input = RadonArray::from(vec![
        RadonInteger::from(2).into(),
        RadonInteger::from(6).into(),
    ]);
    let script = vec![Value::Array(vec![
        Value::Integer(IntegerGreaterThan as i128),
        Value::Integer(4),
    ])];
    let output = filter(&input, &script).unwrap();

    let expected = RadonTypes::Array(RadonArray::from(vec![RadonInteger::from(6).into()]));

    assert_eq!(output, expected)
}

#[test]
fn test_filter_negative() {
    use crate::operators::RadonOpCodes::IntegerMultiply;
    use crate::types::integer::RadonInteger;

    let input = RadonArray::from(vec![
        RadonInteger::from(2).into(),
        RadonInteger::from(6).into(),
    ]);
    let script = vec![Value::Array(vec![
        Value::Integer(IntegerMultiply as i128),
        Value::Integer(4),
    ])];
    let result = filter(&input, &script);

    assert_eq!(
        &result.unwrap_err().to_string(),
        "ArrayFilter subscript output was not RadonBoolean (was `RadonTypes::RadonInteger(8)`)"
    );
}

#[test]
fn test_sort_map_string_values() {
    use crate::operators::RadonOpCodes::{BytesAsString, MapGet};
    use crate::types::{bytes::RadonBytes, map::RadonMap};
    use std::collections::HashMap;
    let mut map1 = HashMap::new();
    map1.insert(
        "key1".to_string(),
        RadonBytes::from(Value::Text("value1".to_string())),
    );
    map1.insert(
        "key2".to_string(),
        RadonBytes::from(Value::Text("B".to_string())),
    );

    let mut map2 = HashMap::new();

    map2.insert(
        "key1".to_string(),
        RadonBytes::from(Value::Text("value1".to_string())),
    );
    map2.insert(
        "key2".to_string(),
        RadonBytes::from(Value::Text("A".to_string())),
    );

    let mut map3 = HashMap::new();

    map3.insert(
        "key1".to_string(),
        RadonBytes::from(Value::Text("value1".to_string())),
    );
    map3.insert(
        "key2".to_string(),
        RadonBytes::from(Value::Text("C".to_string())),
    );

    let input = RadonArray::from(vec![
        RadonMap::from(map1.clone()).into(),
        RadonMap::from(map2.clone()).into(),
        RadonMap::from(map3.clone()).into(),
    ]);
    let script = vec![
        Value::Array(vec![
            Value::Integer(MapGet as i128),
            Value::Text("key2".to_string()),
        ]),
        Value::Integer(BytesAsString as i128),
    ];
    let output = sort(&input, &script).unwrap();

    let expected = RadonArray::from(vec![
        RadonMap::from(map2.clone()).into(),
        RadonMap::from(map1.clone()).into(),
        RadonMap::from(map3.clone()).into(),
    ]);

    assert_eq!(output, expected)
}

#[test]
fn test_sort_map_integer_values() {
    use crate::operators::RadonOpCodes::{BytesAsInteger, MapGet};
    use crate::types::{bytes::RadonBytes, map::RadonMap};
    use std::collections::HashMap;
    let mut map1 = HashMap::new();
    map1.insert("key1".to_string(), RadonBytes::from(Value::Integer(0)));
    map1.insert("key2".to_string(), RadonBytes::from(Value::Integer(1)));

    let mut map2 = HashMap::new();

    map2.insert("key1".to_string(), RadonBytes::from(Value::Integer(0)));
    map2.insert("key2".to_string(), RadonBytes::from(Value::Integer(2)));

    let mut map3 = HashMap::new();

    map3.insert("key1".to_string(), RadonBytes::from(Value::Integer(0)));
    map3.insert("key2".to_string(), RadonBytes::from(Value::Integer(-6)));

    let input = RadonArray::from(vec![
        RadonMap::from(map1.clone()).into(),
        RadonMap::from(map2.clone()).into(),
        RadonMap::from(map3.clone()).into(),
    ]);
    let script = vec![
        Value::Array(vec![
            Value::Integer(MapGet as i128),
            Value::Text("key2".to_string()),
        ]),
        Value::Integer(BytesAsInteger as i128),
    ];
    let output = sort(&input, &script).unwrap();

    let expected = RadonArray::from(vec![
        RadonMap::from(map3.clone()).into(),
        RadonMap::from(map1.clone()).into(),
        RadonMap::from(map2.clone()).into(),
    ]);

    assert_eq!(output, expected)
}

#[test]
fn test_sort_idecntial_maps_integer_values() {
    use crate::operators::RadonOpCodes::{BytesAsInteger, MapGet};
    use crate::types::{bytes::RadonBytes, map::RadonMap};
    use std::collections::HashMap;
    let mut map1 = HashMap::new();
    map1.insert("key1".to_string(), RadonBytes::from(Value::Integer(1)));
    map1.insert("key2".to_string(), RadonBytes::from(Value::Integer(1)));

    let mut map2 = HashMap::new();

    map2.insert("key1".to_string(), RadonBytes::from(Value::Integer(2)));
    map2.insert("key2".to_string(), RadonBytes::from(Value::Integer(1)));

    let mut map3 = HashMap::new();

    map3.insert("key1".to_string(), RadonBytes::from(Value::Integer(3)));
    map3.insert("key2".to_string(), RadonBytes::from(Value::Integer(1)));

    let input = RadonArray::from(vec![
        RadonMap::from(map1.clone()).into(),
        RadonMap::from(map2.clone()).into(),
        RadonMap::from(map3.clone()).into(),
    ]);
    let script = vec![
        Value::Array(vec![
            Value::Integer(MapGet as i128),
            Value::Text("key2".to_string()),
        ]),
        Value::Integer(BytesAsInteger as i128),
    ];
    let output = sort(&input, &script).unwrap();

    let expected = RadonArray::from(vec![
        RadonMap::from(map1.clone()).into(),
        RadonMap::from(map2.clone()).into(),
        RadonMap::from(map3.clone()).into(),
    ]);

    assert_eq!(output, expected)
}

#[test]
fn test_sort_empty_map() {
    use crate::operators::RadonOpCodes::{BytesAsInteger, MapGet};
    use crate::types::map::RadonMap;
    use std::collections::HashMap;
    let map1 = HashMap::new();
    let map2 = HashMap::new();
    let map3 = HashMap::new();

    let input = RadonArray::from(vec![
        RadonMap::from(map1.clone()).into(),
        RadonMap::from(map2.clone()).into(),
        RadonMap::from(map3.clone()).into(),
    ]);
    let script = vec![
        Value::Array(vec![
            Value::Integer(MapGet as i128),
            Value::Text("key2".to_string()),
        ]),
        Value::Integer(BytesAsInteger as i128),
    ];
    let output = sort(&input, &script).unwrap_err();

    assert_eq!(output.to_string(), "Failed to get key `key2` from RadonMap")
}

#[test]
fn test_sort_maps_without_byte_decoder() {
    use crate::operators::RadonOpCodes::MapGet;
    use crate::types::{bytes::RadonBytes, map::RadonMap};
    use std::collections::HashMap;
    let mut map1 = HashMap::new();

    map1.insert("key1".to_string(), RadonBytes::from(Value::Integer(0)));
    map1.insert("key2".to_string(), RadonBytes::from(Value::Integer(1)));
    let input = RadonArray::from(vec![RadonMap::from(map1.clone()).into()]);
    let script = vec![Value::Array(vec![
        Value::Integer(MapGet as i128),
        Value::Text("key2".to_string()),
    ])];
    let output = sort(&input, &script).unwrap_err();

    assert_eq!(
        output.to_string(),
        "ArraySort is not supported for RadonArray with inner type `RadonBytes`"
    )
}

#[test]
fn test_sort_map_wrong_decode() {
    use crate::operators::RadonOpCodes::{BytesAsString, MapGet};
    use crate::types::{bytes::RadonBytes, map::RadonMap};
    use std::collections::HashMap;
    let mut map1 = HashMap::new();
    map1.insert("key1".to_string(), RadonBytes::from(Value::Integer(0)));
    map1.insert("key2".to_string(), RadonBytes::from(Value::Integer(1)));

    let input = RadonArray::from(vec![RadonMap::from(map1.clone()).into()]);
    let script = vec![
        Value::Array(vec![
            Value::Integer(MapGet as i128),
            Value::Text("key2".to_string()),
        ]),
        Value::Integer(BytesAsString as i128),
    ];
    let output = sort(&input, &script).unwrap_err();

    assert_eq!(
        output.to_string(),
        "Failed to decode RadonString from serde_cbor::value::Value"
    )
}

#[test]
fn test_sort_map_floats_value() {
    use crate::operators::RadonOpCodes::{BytesAsFloat, MapGet};
    use crate::types::{bytes::RadonBytes, map::RadonMap};
    use std::collections::HashMap;
    let mut map1 = HashMap::new();
    map1.insert(
        "key1".to_string(),
        RadonBytes::from(Value::Float(std::f64::consts::PI)),
    );
    map1.insert(
        "key2".to_string(),
        RadonBytes::from(Value::Float(std::f64::consts::PI)),
    );

    let input = RadonArray::from(vec![RadonMap::from(map1.clone()).into()]);
    let script = vec![
        Value::Array(vec![
            Value::Integer(MapGet as i128),
            Value::Text("key2".to_string()),
        ]),
        Value::Integer(BytesAsFloat as i128),
    ];
    let output = sort(&input, &script).unwrap_err();

    assert_eq!(
        output.to_string(),
        "ArraySort is not supported for RadonArray with inner type `RadonFloat`"
    )
}

#[test]
fn test_sort_string_2_arrays() {
    use crate::types::string::RadonString;

    let input = RadonArray::from(vec![
        RadonString::from("Hello world!").into(),
        RadonString::from("Bye world!").into(),
    ]);
    let expected = RadonArray::from(vec![
        RadonString::from("Bye world!").into(),
        RadonString::from("Hello world!").into(),
    ]);
    let output = sort(&input, &[]).unwrap();
    assert_eq!(output, expected);
}

#[test]
fn test_sort_string_5_arrays() {
    use crate::types::string::RadonString;

    let input = RadonArray::from(vec![
        RadonString::from("aa").into(),
        RadonString::from("ba").into(),
        RadonString::from("ab").into(),
        RadonString::from("a").into(),
        RadonString::from("").into(),
    ]);
    let expected = RadonArray::from(vec![
        RadonString::from("").into(),
        RadonString::from("a").into(),
        RadonString::from("aa").into(),
        RadonString::from("ab").into(),
        RadonString::from("ba").into(),
    ]);
    let output = sort(&input, &[]).unwrap();
    assert_eq!(output, expected);
}

#[test]
fn test_sort_string_4_arrays() {
    use crate::types::string::RadonString;

    let input = RadonArray::from(vec![
        RadonString::from("a").into(),
        RadonString::from("Á").into(),
        RadonString::from("á").into(),
        RadonString::from("A").into(),
    ]);
    let expected = RadonArray::from(vec![
        RadonString::from("A").into(),
        RadonString::from("a").into(),
        RadonString::from("Á").into(),
        RadonString::from("á").into(),
    ]);
    let output = sort(&input, &[]).unwrap();
    assert_eq!(output, expected);
}

#[test]
fn test_sort_int_arrays() {
    use crate::types::integer::RadonInteger;

    let input = RadonArray::from(vec![
        RadonInteger::from(2i128).into(),
        RadonInteger::from(1i128).into(),
        RadonInteger::from(-2i128).into(),
        RadonInteger::from(0i128).into(),
    ]);
    let expected = RadonArray::from(vec![
        RadonInteger::from(-2i128).into(),
        RadonInteger::from(0i128).into(),
        RadonInteger::from(1i128).into(),
        RadonInteger::from(2i128).into(),
    ]);
    let output = sort(&input, &[]).unwrap();
    assert_eq!(output, expected);
}

#[test]
fn test_sort_float_arrays() {
    use crate::types::float::RadonFloat;

    let input = RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let output = sort(&input, &[]).unwrap_err();
    assert_eq!(
        output.to_string(),
        "ArraySort is not supported for RadonArray with inner type `RadonFloat`"
    );
}

#[test]
fn test_sort_non_homogeneous_array() {
    use crate::types::float::RadonFloat;
    use crate::types::integer::RadonInteger;

    let input = RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonInteger::from(2i128).into(),
    ]);
    let output = sort(&input, &[]).unwrap_err();
    assert_eq!(
        output.to_string(),
        "`ArraySort` is not supported for RadonArray with non homogeneous types"
    );
}

#[test]
fn test_sort_empty_array() {
    let input = RadonArray::from(vec![]);
    let expected = RadonArray::from(vec![]);
    let output = sort(&input, &[]).unwrap();
    assert_eq!(output, expected);
}
