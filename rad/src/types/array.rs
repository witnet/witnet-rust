use crate::error::*;
use crate::operators::{identity, Operable, RadonOpCodes};
use crate::script::RadonCall;
use crate::types::{mixed::RadonMixed, RadonType, RadonTypes};

use rmpv::Value;
use std::fmt;
use witnet_data_structures::serializers::decoders::{TryFrom, TryInto};

#[derive(Clone, Debug, PartialEq)]
pub struct RadonArray {
    value: Vec<RadonTypes>,
}

impl<'a> RadonType<'a, Vec<RadonTypes>> for RadonArray {
    fn value(&self) -> Vec<RadonTypes> {
        self.value.clone()
    }
}

impl From<Vec<RadonTypes>> for RadonArray {
    fn from(value: Vec<RadonTypes>) -> Self {
        RadonArray { value }
    }
}

impl TryFrom<Value> for RadonArray {
    type Error = RadError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        value
            .as_array()
            .map(|value_vec| {
                value_vec
                    .iter()
                    .map(|rmpv_value| RadonTypes::try_from(rmpv_value.clone()).ok())
                    .fuse()
                    .flatten()
                    .collect::<Vec<RadonTypes>>()
            })
            .ok_or_else(|| {
                RadError::new(
                    RadErrorKind::EncodeDecode,
                    String::from("Error creating a RadonArray from a MessagePack value"),
                )
            })
            .map(Self::from)
    }
}

impl TryInto<Value> for RadonArray {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        self.value()
            .iter()
            .map(|radon| RadonTypes::try_into(radon.clone()))
            .collect::<Result<Vec<Value>, Self::Error>>()
            .map(Value::from)
    }
}

impl<'a> TryFrom<&'a [u8]> for RadonArray {
    type Error = RadError;

    fn try_from(slice: &'a [u8]) -> Result<Self, Self::Error> {
        let mixed = RadonMixed::try_from(slice);
        let value = mixed.map(|mixed| mixed.value());
        let radon = value.map(Self::try_from);

        match radon {
            Ok(res) => res,
            _ => Err(RadError::new(
                RadErrorKind::EncodeDecode,
                String::from("Failed to decode a RadonArray from bytes"),
            )),
        }
    }
}

/// FIXME: implement
impl<'a> TryInto<Vec<u8>> for RadonArray {
    type Error = RadError;

    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        let value: Result<Value, Self::Error> = self.try_into();
        let slice: Result<Result<Result<Vec<u8>, Self::Error>, Self::Error>, Self::Error> =
            value.map(|value| RadonMixed::try_from(value).map(RadonMixed::try_into));

        match slice {
            Ok(Ok(res)) => res,
            _ => Err(RadError::new(
                RadErrorKind::EncodeDecode,
                String::from("Failed to encode a RadonString into bytes"),
            )),
        }
    }
}

impl fmt::Display for RadonArray {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RadonArray")
    }
}

impl Operable for RadonArray {
    fn operate(self, call: &RadonCall) -> RadResult<RadonTypes> {
        match call {
            (RadonOpCodes::Identity, None) => identity(self.into()),
            (op_code, args) => Err(WitnetError::from(RadError::new(
                RadErrorKind::UnsupportedOperator,
                format!(
                    "Call to {:?} with args {:?} is not supported on type RadonString",
                    op_code, args
                ),
            ))),
        }
    }
}
