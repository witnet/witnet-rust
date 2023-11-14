use std::{
    convert::{TryFrom, TryInto},
    fmt,
};

use serde_cbor::value::{from_value, Value};
use witnet_data_structures::{chain::tapi::ActiveWips, radon_report::ReportContext};

use crate::{
    error::RadError,
    operators::{identity, string as string_operators, Operable, RadonOpCodes},
    script::RadonCall,
    types::{RadonType, RadonTypes},
};

const RADON_STRING_TYPE_NAME: &str = "RadonString";

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct RadonString {
    value: String,
}

impl RadonType<String> for RadonString {
    fn value(&self) -> String {
        self.value.clone()
    }

    #[inline]
    fn radon_type_name() -> &'static str {
        RADON_STRING_TYPE_NAME
    }
}

impl TryFrom<Value> for RadonString {
    type Error = RadError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        from_value::<String>(value)
            .map(Self::from)
            .map_err(|_| RadError::Decode {
                from: "serde_cbor::value::Value",
                to: RadonString::radon_type_name(),
            })
    }
}

impl TryFrom<RadonTypes> for RadonString {
    type Error = RadError;

    fn try_from(item: RadonTypes) -> Result<Self, Self::Error> {
        let original_type = item.radon_type_name();

        match item {
            RadonTypes::Boolean(rad_bool) => Ok(RadonString::from(rad_bool.value().to_string())),
            RadonTypes::Float(rad_float) => Ok(RadonString::from(rad_float.value().to_string())),
            RadonTypes::Integer(rad_integer) => {
                Ok(RadonString::from(rad_integer.value().to_string()))
            }
            RadonTypes::String(rad_string) => Ok(rad_string),
            item => Value::try_from(item)?
                .try_into()
                .map_err(|_| Self::Error::Decode {
                    from: original_type,
                    to: RadonString::radon_type_name(),
                }),
        }
    }
}

impl TryInto<Value> for RadonString {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        Ok(Value::from(self.value()))
    }
}

impl From<String> for RadonString {
    fn from(value: String) -> Self {
        RadonString { value }
    }
}

impl<'a> From<&'a str> for RadonString {
    fn from(value: &'a str) -> Self {
        Self::from(String::from(value))
    }
}

impl Operable for RadonString {
    fn operate(&self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        self.operate_in_context(call, &mut ReportContext::default())
    }

    fn operate_in_context(
        &self,
        call: &RadonCall,
        context: &mut ReportContext<RadonTypes>,
    ) -> Result<RadonTypes, RadError> {
        let wip0024 = context
            .active_wips
            .as_ref()
            .map(ActiveWips::wip0024)
            .unwrap_or(true);

        match call {
            (RadonOpCodes::Identity, None) => identity(RadonTypes::from(self.clone())),
            (RadonOpCodes::StringAsFloat, args) => if wip0024 {
                string_operators::as_float(self, args)
            } else {
                string_operators::legacy::as_float_before_wip0024(self)
            }
            .map(RadonTypes::from)
            .map_err(Into::into),
            (RadonOpCodes::StringAsInteger, args) => if wip0024 {
                string_operators::as_integer(self, args)
            } else {
                string_operators::legacy::as_integer_before_wip0024(self)
            }
            .map(RadonTypes::from)
            .map_err(Into::into),
            (RadonOpCodes::StringAsBoolean, None) => string_operators::to_bool(self)
                .map(RadonTypes::from)
                .map_err(Into::into),
            (RadonOpCodes::StringParseJSONArray, None) => string_operators::parse_json_array(self)
                .map(RadonTypes::from)
                .map_err(Into::into),
            (RadonOpCodes::StringParseJSONMap, None) => string_operators::parse_json_map(self)
                .map(RadonTypes::from)
                .map_err(Into::into),
            (RadonOpCodes::StringMatch, Some(args)) => {
                string_operators::string_match(self, args.as_slice()).map(RadonTypes::from)
            }
            (RadonOpCodes::StringLength, None) => {
                Ok(RadonTypes::from(string_operators::length(self)))
            }
            (RadonOpCodes::StringToLowerCase, None) => {
                Ok(RadonTypes::from(string_operators::to_lowercase(self)))
            }
            (RadonOpCodes::StringToUpperCase, None) => {
                Ok(RadonTypes::from(string_operators::to_uppercase(self)))
            }
            (RadonOpCodes::StringParseXMLMap, None) => string_operators::parse_xml_map(self)
                .map(RadonTypes::from)
                .map_err(Into::into),
            (RadonOpCodes::StringReplace, Some(args)) => {
                string_operators::string_replace(self, args.as_slice()).map(RadonTypes::from).map_err(Into::into)
            }
            (RadonOpCodes::StringSlice, Some(args)) => {
                string_operators::string_slice(self, args.as_slice()).map(RadonTypes::from).map_err(Into::into)
            }
            (RadonOpCodes::StringSplit, Some(args)) => {
                string_operators::string_split(self, args.as_slice()).map(RadonTypes::from).map_err(Into::into)
            }
            (op_code, args) => Err(RadError::UnsupportedOperator {
                input_type: RADON_STRING_TYPE_NAME.to_string(),
                operator: op_code.to_string(),
                args: args.to_owned(),
            }),
        }
    }
}

impl fmt::Display for RadonString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, r#"{}("{}")"#, RADON_STRING_TYPE_NAME, self.value)
    }
}

#[test]
fn test_operate_identity() {
    let input = RadonString::from("Hello world!");
    let expected = RadonString::from("Hello world!").into();

    let call = (RadonOpCodes::Identity, None);
    let output = input.operate(&call).unwrap();

    assert_eq!(output, expected);
}

#[test]
fn test_operate_unimplemented() {
    let input = RadonString::from("Hello world!");

    let call = (RadonOpCodes::Fail, None);
    let result = input.operate(&call);

    assert!(result.is_err());
}

#[test]
fn test_serialize_radon_string() {
    use witnet_data_structures::radon_report::TypeLike;

    let input = RadonTypes::from(RadonString::from("Hello world!"));
    let expected: Vec<u8> = vec![108, 72, 101, 108, 108, 111, 32, 119, 111, 114, 108, 100, 33];

    let output = input.encode().unwrap();

    assert_eq!(output, expected);
}
