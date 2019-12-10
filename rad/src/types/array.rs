use std::{
    convert::{TryFrom, TryInto},
    fmt,
    mem::{discriminant, Discriminant},
};

use serde::ser::{Serialize, SerializeStruct, Serializer};
use serde_cbor::value::{from_value, Value};

use witnet_data_structures::radon_report::{ReportContext, Stage};

use crate::{
    error::RadError,
    operators::{
        array as array_operators, check_valid_operator_for_tally_stage, identity, Operable,
        RadonOpCodes,
    },
    script::RadonCall,
    types::{
        float::RadonFloat, map::RadonMap, mixed::RadonMixed, string::RadonString, RadonType,
        RadonTypes,
    },
};

fn bytes_discriminant() -> Discriminant<RadonTypes> {
    discriminant(&RadonTypes::from(RadonMixed::from(Value::Null)))
}

pub const RADON_ARRAY_TYPE_NAME: &str = "RadonArray";

#[derive(Clone, Debug, PartialEq)]
pub struct RadonArray {
    value: Vec<RadonTypes>,
    inner_type: Discriminant<RadonTypes>,
}

impl RadonArray {
    pub fn inner_type(&self) -> Discriminant<RadonTypes> {
        self.inner_type
    }

    pub fn is_homogeneous(&self) -> bool {
        self.inner_type != bytes_discriminant()
    }
}

impl Serialize for RadonArray {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("RadonArray", 2)?;

        state.serialize_field("value", &self.value)?;

        if self.inner_type() == discriminant(&RadonTypes::Float(RadonFloat::default())) {
            state.serialize_field("inner_type", "RadonFloat")?;
        } else if self.inner_type() == discriminant(&RadonTypes::Map(RadonMap::default())) {
            state.serialize_field("inner_type", "RadonMap")?;
        } else if self.inner_type() == discriminant(&RadonTypes::Mixed(RadonMixed::default()))
            || self.inner_type() == discriminant(&RadonTypes::String(RadonString::default()))
        {
            state.serialize_field("inner_type", "RadonBytes")?;
        } else {
            state.serialize_field("inner_value", "RadonArray")?;
        }

        state.end()
    }
}

impl RadonType<Vec<RadonTypes>> for RadonArray {
    fn value(&self) -> Vec<RadonTypes> {
        self.value.clone()
    }

    fn radon_type_name() -> String {
        RADON_ARRAY_TYPE_NAME.to_string()
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
            .unwrap_or_else(bytes_discriminant);

        RadonArray { value, inner_type }
    }
}

impl TryFrom<Value> for RadonArray {
    type Error = RadError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        from_value::<Vec<Value>>(value)
            .map(|value_vec| {
                value_vec
                    .iter()
                    .map(|cbor_value| RadonTypes::try_from(cbor_value.to_owned()).ok())
                    .fuse()
                    .flatten()
                    .collect::<Vec<RadonTypes>>()
            })
            .map_err(|_| RadError::Decode {
                from: "cbor::Value".to_string(),
                to: RADON_ARRAY_TYPE_NAME.to_string(),
            })
            .map(Self::from)
    }
}

impl TryInto<Value> for RadonArray {
    type Error = RadError;

    fn try_into(self) -> Result<Value, Self::Error> {
        self.value()
            .iter()
            .map(|radon| RadonTypes::try_into(radon.to_owned()))
            .collect::<Result<Vec<Value>, Self::Error>>()
            .map(Value::from)
    }
}

impl fmt::Display for RadonArray {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}({:?})", RADON_ARRAY_TYPE_NAME, self.value)
    }
}

impl Operable for RadonArray {
    fn operate(&self, call: &RadonCall) -> Result<RadonTypes, RadError> {
        match call {
            (RadonOpCodes::Identity, None) => identity(RadonTypes::from(self.clone())),
            (RadonOpCodes::ArrayCount, None) => Ok(array_operators::count(self).into()),
            (RadonOpCodes::Get, Some(args)) => array_operators::get(self, args.as_slice()),
            (RadonOpCodes::ArrayGet, Some(args)) => array_operators::get(self, args.as_slice()),
            (RadonOpCodes::ArrayFilter, Some(args)) => {
                array_operators::filter(self, args.as_slice(), &mut ReportContext::default())
            }
            (RadonOpCodes::ArrayMap, Some(args)) => array_operators::map(self, args.as_slice()),
            (RadonOpCodes::ArrayReduce, Some(args)) => {
                array_operators::reduce(self, args.as_slice(), &mut ReportContext::default())
            }
            (RadonOpCodes::ArraySort, Some(args)) => {
                array_operators::sort(self, args.as_slice()).map(RadonTypes::from)
            }
            (op_code, args) => Err(RadError::UnsupportedOperator {
                input_type: RADON_ARRAY_TYPE_NAME.to_string(),
                operator: op_code.to_string(),
                args: args.to_owned(),
            }),
        }
    }

    fn operate_in_context(
        &self,
        call: &RadonCall,
        context: &mut ReportContext,
    ) -> Result<RadonTypes, RadError> {
        if let Stage::Tally { .. } = context.stage {
            check_valid_operator_for_tally_stage(call)?;
        }
        // Intercept filter operations for performing the filters in a context, otherwise use
        // context-free execution.
        match call {
            (RadonOpCodes::ArrayFilter, Some(args)) => {
                array_operators::filter(self, args.as_slice(), context)
            }
            (RadonOpCodes::ArrayReduce, Some(args)) => {
                array_operators::reduce(self, args.as_slice(), context)
            }
            other => self.operate(other),
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
fn test_operate_count() {
    use crate::types::{integer::RadonInteger, string::RadonString};

    let input1 = RadonArray::from(vec![
        RadonString::from("Hello!").into(),
        RadonString::from("world!").into(),
    ]);
    let expected1 = RadonInteger::from(2).into();

    let input2 = RadonArray::from(vec![]);
    let expected2 = RadonInteger::from(0).into();

    let call = (RadonOpCodes::ArrayCount, None);

    let output1 = input1.operate(&call).unwrap();
    assert_eq!(output1, expected1);

    let output2 = input2.operate(&call).unwrap();
    assert_eq!(output2, expected2);
}

#[test]
fn test_operate_reduce_average_mean_float() {
    use crate::types::float::RadonFloat;

    let input = RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let call = (RadonOpCodes::ArrayReduce, Some(vec![Value::Integer(0x03)]));
    let expected = RadonTypes::from(RadonFloat::from(1.5f64));

    let output = input.operate(&call).unwrap();

    assert_eq!(output, expected);
}

#[test]
fn test_operate_map_float_multiply() {
    let input = RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let call = (
        RadonOpCodes::ArrayMap,
        Some(vec![
            Value::Array(vec![Value::Array(vec![
                Value::Integer(0x38),
                Value::Integer(2i128),
            ])]), // [ OP_FLOAT_MULTIPLY, 2 ]
        ]),
    );
    let expected = RadonTypes::from(RadonArray::from(vec![
        RadonFloat::from(2f64).into(),
        RadonFloat::from(4f64).into(),
    ]));

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
    use witnet_data_structures::radon_report::TypeLike;

    let input = RadonTypes::from(RadonArray::from(vec![
        RadonString::from("Hello").into(),
        RadonString::from("world!").into(),
    ]));
    let expected: Vec<u8> = vec![
        130, 101, 72, 101, 108, 108, 111, 102, 119, 111, 114, 108, 100, 33,
    ];

    let output = input.encode().unwrap();

    assert_eq!(output, expected);
}
