use crate::error::*;
use crate::types::{mixed::RadonMixed, string::RadonString, RadonType};

use json;
use rmpv;
use std::error::Error;

pub fn parse_json(input: &RadonString) -> RadResult<RadonMixed> {
    match json::parse(&input.value()) {
        Ok(json_value) => {
            let value = json_to_rmp(&json_value);
            Ok(RadonMixed::from(value.to_owned()))
        }
        Err(json_error) => Err(WitnetError::from(RadError::new(
            RadErrorKind::JsonParse,
            json_error.description().to_owned(),
        ))),
    }
}

fn json_to_rmp(value: &json::JsonValue) -> rmpv::ValueRef {
    match value {
        json::JsonValue::Array(value) => {
            rmpv::ValueRef::Array(value.iter().map(json_to_rmp).collect())
        }
        json::JsonValue::Object(value) => {
            let entries = value
                .iter()
                .map(|(key, value)| (rmpv::ValueRef::from(key), json_to_rmp(value)))
                .collect();
            rmpv::ValueRef::Map(entries)
        }
        json::JsonValue::Short(value) => {
            rmpv::ValueRef::String(rmpv::Utf8StringRef::from(value.as_str()))
        }
        json::JsonValue::String(value) => {
            rmpv::ValueRef::String(rmpv::Utf8StringRef::from(value.as_str()))
        }
        json::JsonValue::Number(value) => rmpv::ValueRef::F64((*value).into()),
        _ => rmpv::ValueRef::Nil,
    }
}

#[test]
fn test_parse_json() {
    let valid_string = RadonString::from(r#"{ "Hello": "world" }"#);
    let invalid_string = RadonString::from(r#"{ "Hello": }"#);

    let valid_object = parse_json(&valid_string).unwrap();
    let invalid_object = parse_json(&invalid_string);

    assert!(if let rmpv::Value::Map(vector) = valid_object.value() {
        if let Some((rmpv::Value::String(key), rmpv::Value::String(val))) = vector.first() {
            key.as_str() == Some("Hello") && val.as_str() == Some("world")
        } else {
            false
        }
    } else {
        false
    });

    assert!(if let Err(_error) = invalid_object {
        true
    } else {
        false
    });
}
