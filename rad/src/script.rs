use crate::error::*;
use crate::operators::{operate, RadonOpCodes};
use crate::types::RadonTypes;

use log::error;
use num_traits::FromPrimitive;
use rmpv::{self, Value};
use std::{error::Error, io::Cursor};
use witnet_util::error::WitnetError;

pub type RadonCall = (RadonOpCodes, Option<Vec<Value>>);

pub type RadonScript = Vec<RadonCall>;

/// Run any RADON script on given input data.
pub fn execute_radon_script(input: RadonTypes, script: &'_ [RadonCall]) -> RadResult<RadonTypes> {
    script.iter().try_fold(input, operate)
}

pub fn unpack_radon_script(packed: &[u8]) -> RadResult<RadonScript> {
    let reader = &mut Cursor::new(packed);

    match rmpv::decode::value::read_value(reader) {
        Ok(Value::Array(array)) => array
            .iter()
            .map(unpack_radon_call)
            .collect::<RadResult<RadonScript>>(),
        Ok(other) => Err(errorify(
            RadErrorKind::ScriptNotArray,
            &format!("Script is not Array but {:}", other),
        )),
        Err(error) => Err(errorify(RadErrorKind::MessagePack, error.description())),
    }
}

fn unpack_radon_call(packed_call: &Value) -> RadResult<RadonCall> {
    match packed_call {
        Value::Array(array) => unpack_compound_call(array),
        Value::Integer(integer) => integer.as_u64().map_or_else(
            || {
                Err(errorify(
                    RadErrorKind::NotNaturalOperator,
                    &format!(
                        "The given operator code ({:?}) is not a natural Integer",
                        integer
                    ),
                ))
            },
            |natural| {
                RadonOpCodes::from_u64(natural).map_or_else(
                    || {
                        Err(errorify(
                            RadErrorKind::UnknownOperator,
                            &format!("The given operator code ({:?}) is unknown", natural),
                        ))
                    },
                    |op_code| Ok((op_code, None)),
                )
            },
        ),
        code => Err(errorify(
            RadErrorKind::NotIntegerOperator,
            &format!(
                "The given operator code ({:?}) is not a valid Integer",
                code
            ),
        )),
    }
}

fn unpack_compound_call(array: &[Value]) -> RadResult<RadonCall> {
    array
        .split_first()
        .ok_or_else(|| {
            errorify(
                RadErrorKind::NoOperatorInCompoundCall,
                "No operator found in compound call",
            )
        })
        .map(|(head, tail)| {
            head.as_u64()
                .map(RadonOpCodes::from_u64)
                .unwrap_or(None)
                .map(|op_code| (op_code, Some(tail.to_vec())))
                .ok_or_else(|| {
                    errorify(
                        RadErrorKind::NotIntegerOperator,
                        "The given operator code is not a valid Integer",
                    )
                })
        })
        .unwrap_or_else(Err)
}

fn errorify(kind: RadErrorKind, message: &str) -> WitnetError<RadError> {
    error!("{} while unpacking a RADON script: {}", kind, message);

    WitnetError::from(RadError::new(kind, String::from(message)))
}

#[test]
fn test_execute_radon_script() {
    use crate::types::{string::RadonString, RadonType};

    let input = RadonString::from(r#"{ "Hello": "world" }"#).into();
    let script = vec![
        (RadonOpCodes::Identity, None),
        (RadonOpCodes::ParseJson, None),
        (RadonOpCodes::Identity, None),
    ];
    let output = execute_radon_script(input, &script);

    assert!(if let Ok(RadonTypes::Mixed(mixed)) = output {
        if let rmpv::Value::Map(vector) = mixed.value() {
            if let Some((rmpv::Value::String(key), rmpv::Value::String(val))) = vector.first() {
                key.as_str() == Some("Hello") && val.as_str() == Some("world")
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    });
}

#[test]
fn test_unpack_radon_script() {
    let packed = [147, 0, 83, 0];
    let expected = vec![
        (RadonOpCodes::Identity, None),
        (RadonOpCodes::ParseJson, None),
        (RadonOpCodes::Identity, None),
    ];

    let output = unpack_radon_script(&packed);

    assert!(if let Ok(script) = output {
        script == expected
    } else {
        false
    });
}
