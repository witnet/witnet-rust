use crate::error::RadError;
use crate::operators::{operate, RadonOpCodes};
use crate::types::RadonTypes;

use log::error;
use num_traits::FromPrimitive;
use rmpv::{self, Value};
use std::{error::Error, io::Cursor};

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
    let reader = &mut Cursor::new(packed);

    match rmpv::decode::value::read_value(reader) {
        Ok(Value::Array(array)) => array
            .iter()
            .map(unpack_radon_call)
            .collect::<Result<RadonScript, RadError>>(),
        Ok(other) => Err(errorify(RadError::ScriptNotArray {
            input_type: other.to_string(),
        })),
        Err(error) => Err(errorify(RadError::MessagePack {
            description: error.description().to_string(),
        })),
    }
}

pub fn unpack_radon_call(packed_call: &Value) -> Result<RadonCall, RadError> {
    match packed_call {
        Value::Array(array) => unpack_compound_call(array),
        Value::Integer(integer) => integer.as_u64().map_or_else(
            || Err(errorify(RadError::NotNaturalOperator { code: *integer })),
            |natural| {
                RadonOpCodes::from_u64(natural).map_or_else(
                    || Err(errorify(RadError::UnknownOperator { code: natural })),
                    |op_code| Ok((op_code, None)),
                )
            },
        ),
        code => Err(errorify(RadError::NotIntegerOperator {
            code: Box::new(code.clone()),
        })),
    }
}

fn unpack_compound_call(array: &[Value]) -> Result<RadonCall, RadError> {
    array
        .split_first()
        .ok_or_else(|| errorify(RadError::NoOperatorInCompoundCall))
        .map(|(head, tail)| {
            head.as_u64()
                .map(RadonOpCodes::from_u64)
                .unwrap_or(None)
                .map(|op_code| (op_code, Some(tail.to_vec())))
                .ok_or_else(|| {
                    errorify(RadError::NotIntegerOperator {
                        code: Box::new(head.clone()),
                    })
                })
        })
        .unwrap_or_else(Err)
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
        (RadonOpCodes::StringParseJson, None),
        (RadonOpCodes::MixedToMap, None),
        (RadonOpCodes::Get, Some(vec![Value::from("main")])),
        (RadonOpCodes::MixedToMap, None),
        (RadonOpCodes::Get, Some(vec![Value::from("temp")])),
        (RadonOpCodes::MixedToFloat, None),
    ];
    let output = execute_radon_script(input, &script).unwrap();

    let expected = RadonTypes::Float(RadonFloat::from(-4f64));

    assert_eq!(output, expected)
}

#[test]
fn test_unpack_radon_script() {
    let packed = [
        150, 67, 116, 146, 1, 164, 109, 97, 105, 110, 116, 146, 1, 164, 116, 101, 109, 112, 114,
    ];
    let expected = vec![
        (RadonOpCodes::StringParseJson, None),
        (RadonOpCodes::MixedToMap, None),
        (RadonOpCodes::Get, Some(vec![Value::from("main")])),
        (RadonOpCodes::MixedToMap, None),
        (RadonOpCodes::Get, Some(vec![Value::from("temp")])),
        (RadonOpCodes::MixedToFloat, None),
    ];
    println!("{:?}", expected);

    let output = unpack_radon_script(&packed).unwrap();

    assert_eq!(output, expected)
}
