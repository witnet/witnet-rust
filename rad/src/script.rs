use log::error;
use num_traits::FromPrimitive;
use serde_cbor::{
    self as cbor,
    value::{from_value, Value},
};

use crate::error::RadError;
use crate::operators::{operate, RadonOpCodes};
use crate::types::RadonTypes;

pub type RadonCall = (RadonOpCodes, Option<Vec<Value>>);

pub type RadonScript = Vec<RadonCall>;

/// Run any RADON script on given input data.
pub fn execute_radon_script(
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
                RadonOpCodes::from_i8(*integer as i8).map_or_else(
                    || Err(errorify(RadError::UnknownOperator { code: *integer })),
                    |op_code| Ok((op_code, None)),
                )
            } else {
                Err(errorify(RadError::NotNaturalOperator { code: *integer }))
            }
        }
        _ => Err(errorify(RadError::NotIntegerOperator)),
    }
}

fn unpack_compound_call(array: &[Value]) -> Result<RadonCall, RadError> {
    array
        .split_first()
        .ok_or_else(|| errorify(RadError::NoOperatorInCompoundCall))
        .map(|(head, tail)| {
            from_value::<i8>(head.to_owned())
                .map(RadonOpCodes::from_i8)
                .unwrap_or(None)
                .map(|op_code| (op_code, Some(tail.to_vec())))
                .ok_or_else(|| errorify(RadError::NotIntegerOperator))
        })
        .unwrap_or_else(Err)
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
        (RadonOpCodes::BytesAsMap, None),
        (
            RadonOpCodes::Get,
            Some(vec![Value::Text(String::from("main"))]),
        ),
        (RadonOpCodes::BytesAsMap, None),
        (
            RadonOpCodes::Get,
            Some(vec![Value::Text(String::from("temp"))]),
        ),
        (RadonOpCodes::BytesAsFloat, None),
    ];
    let output = execute_radon_script(input, &script).unwrap();

    let expected = RadonTypes::Float(RadonFloat::from(-4f64));

    assert_eq!(output, expected)
}

#[test]
fn test_unpack_radon_script() {
    let packed = [
        134, 24, 69, 24, 116, 130, 1, 100, 109, 97, 105, 110, 24, 116, 130, 1, 100, 116, 101, 109,
        112, 24, 114,
    ];
    let expected = vec![
        (RadonOpCodes::StringParseJSON, None),
        (RadonOpCodes::BytesAsMap, None),
        (
            RadonOpCodes::Get,
            Some(vec![Value::Text(String::from("main"))]),
        ),
        (RadonOpCodes::BytesAsMap, None),
        (
            RadonOpCodes::Get,
            Some(vec![Value::Text(String::from("temp"))]),
        ),
        (RadonOpCodes::BytesAsFloat, None),
    ];
    println!("{:?}", expected);

    let output = unpack_radon_script(&packed).unwrap();

    assert_eq!(output, expected)
}
