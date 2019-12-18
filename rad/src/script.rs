use std::convert::TryFrom;

use log::error;
use serde_cbor::{
    self as cbor,
    value::{from_value, Value},
};

use witnet_data_structures::radon_report::{RadonReport, ReportContext};

use crate::{
    error::RadError,
    operators::{operate, operate_in_context, RadonOpCodes},
    types::RadonTypes,
};

pub type RadonCall = (RadonOpCodes, Option<Vec<Value>>);
pub type RadonScript = Vec<RadonCall>;

/// Run any RADON script on given input data, and return `RadonReport`.
pub fn execute_radon_script(
    input: RadonTypes,
    script: &[RadonCall],
    context: &mut ReportContext,
) -> Result<RadonReport<RadonTypes>, RadError> {
    // Set the execution timestamp
    context.start();
    // Run the execution
    let result = script
        .iter()
        .enumerate()
        .try_fold(input, |input, (i, call)| {
            context.call_index = Some(i as u8);
            operate_in_context(input, call, context)
        });
    // Set the completion timestamp
    context.complete();

    // Return a report as constructed from the result and the context
    RadonReport::from_result(result, context)
}

/// Run any RADON script on given input data, and return `RadonTypes`.
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
                RadonOpCodes::try_from(*integer as u8)
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
    error!("Error unpacking a RADON script: {:?}", kind);

    kind
}

#[test]
fn test_execute_radon_script() {
    use crate::types::{float::RadonFloat, string::RadonString};

    let input = RadonString::from(r#"{"coord":{"lon":13.41,"lat":52.52},"weather":[{"id":600,"main":"Snow","description":"light snow","icon":"13n"}],"base":"stations","main":{"temp":-4,"pressure":1013,"humidity":73,"temp_min":-4,"temp_max":-4},"visibility":10000,"wind":{"speed":2.6,"deg":90},"clouds":{"all":75},"dt":1548346800,"sys":{"type":1,"id":1275,"message":0.0038,"country":"DE","sunrise":1548313160,"sunset":1548344298},"id":2950159,"name":"Berlin","cod":200}"#).into();
    let script = vec![
        (RadonOpCodes::StringParseJSON, None),
        (RadonOpCodes::MixedAsMap, None),
        (
            RadonOpCodes::MapGetMap,
            Some(vec![Value::Text(String::from("main"))]),
        ),
        (
            RadonOpCodes::MapGetFloat,
            Some(vec![Value::Text(String::from("temp"))]),
        ),
    ];
    let output = execute_contextfree_radon_script(input, &script).unwrap();

    let expected = RadonTypes::Float(RadonFloat::from(-4f64));

    assert_eq!(output, expected)
}

#[test]
fn test_unpack_radon_script() {
    let cbor_vec = Value::Array(vec![
        Value::Integer(RadonOpCodes::StringParseJSON as i128),
        Value::Integer(RadonOpCodes::MixedAsMap as i128),
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
        (RadonOpCodes::StringParseJSON, None),
        (RadonOpCodes::MixedAsMap, None),
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
