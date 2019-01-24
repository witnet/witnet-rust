use crate::error::*;
use crate::operators::{array as array_operators, identity, Operable, RadonOpCodes};
use crate::script::RadonCall;
use crate::types::{mixed::RadonMixed, RadonType, RadonTypes};

use rmpv::Value;
use std::fmt;
use std::mem::{discriminant, Discriminant};
use witnet_data_structures::serializers::decoders::{TryFrom, TryInto};

fn mixed_discriminant() -> Discriminant<RadonTypes> {
    discriminant(&RadonTypes::from(RadonMixed::from(Value::Nil)))
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadonArray {
    value: Vec<RadonTypes>,
    inner_type: Discriminant<RadonTypes>,
}

impl RadonArray {
    pub fn is_homogeneous(&self) -> bool {
        self.inner_type != mixed_discriminant()
    }
}

impl<'a> RadonType<'a, Vec<RadonTypes>> for RadonArray {
    fn value(&self) -> Vec<RadonTypes> {
        self.value.clone()
    }
}

impl From<Vec<RadonTypes>> for RadonArray {
    fn from(value: Vec<RadonTypes>) -> Self {
        let mut iter = value.iter();
        let first_type = iter.nth(0).map(discriminant);

        let inner_type = first_type
            .map_or(first_type, |first_type| {
                iter.try_fold(first_type, |previous_type, current| {
                    let current_type = discriminant(current);

                    if current_type == previous_type {
                        Some(current_type)
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_else(mixed_discriminant);

        RadonArray { value, inner_type }
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
        let mixed = RadonMixed::try_from(slice)?;
        let value: Value = RadonMixed::try_into(mixed)?;

        Self::try_from(value)
    }
}

impl<'a> TryInto<Vec<u8>> for RadonArray {
    type Error = RadError;

    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        let value: Value = Self::try_into(self)?;
        let mixed = RadonMixed::try_from(value)?;

        RadonMixed::try_into(mixed)
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
            (RadonOpCodes::Reduce, Some(args)) => array_operators::reduce(&self, args.as_slice()),
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

#[test]
fn test_operate_identity() {
    use crate::types::string::RadonString;

    let input = RadonArray::from(vec![RadonString::from("Hello world!").into()]);
    let expected = RadonArray::from(vec![RadonString::from("Hello world!").into()]).into();

    let call = (RadonOpCodes::Identity, None);
    let output = input.operate(&call).unwrap();

    assert_eq!(output, expected);
}

#[test]
fn test_operate_reduce_average_mean_float() {
    use crate::types::float::RadonFloat;

    let input = RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let call = (RadonOpCodes::Reduce, Some(vec![Value::from(0x20)]));
    let expected = RadonTypes::from(RadonFloat::from(1.5f64));

    let output = input.operate(&call).unwrap();

    assert_eq!(output, expected);
}

#[test]
fn test_operate_unimplemented() {
    let input = RadonArray::from(vec![]);

    let call = (RadonOpCodes::Fail, None);
    let result = input.operate(&call);

    assert!(if let Err(_error) = result {
        true
    } else {
        false
    });
}

#[test]
fn test_serialize_radon_array() {
    use crate::types::string::RadonString;

    let input = RadonArray::from(vec![
        RadonString::from("Hello").into(),
        RadonString::from("world!").into(),
    ]);
    let expected: Vec<u8> = vec![
        146, 165, 72, 101, 108, 108, 111, 166, 119, 111, 114, 108, 100, 33,
    ];

    let output: Vec<u8> = input.try_into().unwrap();

    assert_eq!(output, expected);
}
