use serde_cbor::value::{from_value, Value};
use std::{
    convert::{TryFrom, TryInto},
    fmt,
};

use witnet_data_structures::radon_report::ReportContext;

use crate::{
    error::RadError,
    operators::{array as array_operators, identity, Operable, RadonOpCodes},
    script::RadonCall,
    types::{RadonType, RadonTypes},
};

const RADON_ARRAY_TYPE_NAME: &str = "RadonArray";

#[derive(Clone, Debug, PartialEq)]
pub struct RadonArray {
    value: Vec<RadonTypes>,
    is_homogeneous: bool,
}

impl RadonArray {
    pub fn is_homogeneous(&self) -> bool {
        self.is_homogeneous
    }
}

impl RadonType<Vec<RadonTypes>> for RadonArray {
    fn value(&self) -> Vec<RadonTypes> {
        self.value.clone()
    }

    #[inline]
    fn radon_type_name() -> &'static str {
        RADON_ARRAY_TYPE_NAME
    }
}

impl From<Vec<RadonTypes>> for RadonArray {
    fn from(value: Vec<RadonTypes>) -> Self {
        let mut iter = value.iter();
        let first_type = iter.next().map(|rad_types| rad_types.discriminant());

        let is_homogeneous = first_type
            .map_or(first_type, |first_type| {
                iter.try_fold(first_type, |previous_type, current| {
                    let current_type = current.discriminant();

                    if current_type == previous_type {
                        Some(current_type)
                    } else {
                        None
                    }
                })
            })
            .is_some();

        RadonArray {
            value,
            is_homogeneous,
        }
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
                from: "cbor::value::Value",
                to: RadonArray::radon_type_name(),
            })
            .map(Self::from)
    }
}

impl TryFrom<RadonTypes> for RadonArray {
    type Error = RadError;

    fn try_from(item: RadonTypes) -> Result<Self, Self::Error> {
        if let RadonTypes::Array(rad_array) = item {
            Ok(rad_array)
        } else {
            let value = Value::try_from(item)?;
            value.try_into()
        }
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
            (RadonOpCodes::ArrayGetArray, Some(args)) => {
                array_operators::get_array(self, args.as_slice()).map(RadonTypes::from)
            }
            (RadonOpCodes::ArrayGetBoolean, Some(args)) => {
                array_operators::get_boolean(self, args.as_slice()).map(RadonTypes::from)
            }
            (RadonOpCodes::ArrayGetBytes, Some(args)) => {
                array_operators::get_bytes(self, args.as_slice()).map(RadonTypes::from)
            }
            (RadonOpCodes::ArrayGetInteger, Some(args)) => {
                array_operators::get_integer(self, args.as_slice()).map(RadonTypes::from)
            }
            (RadonOpCodes::ArrayGetFloat, Some(args)) => {
                array_operators::get_float(self, args.as_slice()).map(RadonTypes::from)
            }
            (RadonOpCodes::ArrayGetMap, Some(args)) => {
                array_operators::get_map(self, args.as_slice()).map(RadonTypes::from)
            }
            (RadonOpCodes::ArrayGetString, Some(args)) => {
                array_operators::get_string(self, args.as_slice()).map(RadonTypes::from)
            }
            (RadonOpCodes::ArrayFilter, Some(args)) => {
                array_operators::filter(self, args.as_slice(), &mut ReportContext::default())
            }
            (RadonOpCodes::ArrayMap, Some(args)) => {
                array_operators::map(self, args.as_slice(), &mut ReportContext::default())
            }
            (RadonOpCodes::ArrayReduce, Some(args)) => {
                array_operators::reduce(self, args.as_slice(), &mut ReportContext::default())
            }
            (RadonOpCodes::ArraySort, Some(args)) => {
                array_operators::sort(self, args.as_slice(), &mut ReportContext::default())
                    .map(RadonTypes::from)
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
        context: &mut ReportContext<RadonTypes>,
    ) -> Result<RadonTypes, RadError> {
        match call {
            (RadonOpCodes::ArrayFilter, Some(args)) => {
                array_operators::filter(self, args.as_slice(), context)
            }
            (RadonOpCodes::ArrayMap, Some(args)) => {
                array_operators::map(self, args.as_slice(), context)
            }
            (RadonOpCodes::ArrayReduce, Some(args)) => {
                array_operators::reduce(self, args.as_slice(), context)
            }
            (RadonOpCodes::ArraySort, Some(args)) => {
                array_operators::sort(self, args.as_slice(), context)
            }
            other => self.operate(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        current_active_wips,
        reducers::RadonReducers,
        types::{
            boolean::RadonBoolean, bytes::RadonBytes, float::RadonFloat, integer::RadonInteger,
            map::RadonMap, string::RadonString,
        },
    };
    use std::collections::BTreeMap;
    use witnet_data_structures::radon_report::TypeLike;

    #[test]
    fn test_operate_identity() {
        let input = RadonArray::from(vec![RadonString::from("Hello world!").into()]);
        let expected = RadonArray::from(vec![RadonString::from("Hello world!").into()]).into();

        let call = (RadonOpCodes::Identity, None);
        let output = input.operate(&call).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_operate_count() {
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
        let input = RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]);
        let call = (
            RadonOpCodes::ArrayReduce,
            Some(vec![Value::Integer(RadonReducers::AverageMean as i128)]),
        );
        let expected = RadonTypes::from(RadonFloat::from(1.5f64));

        let output = input.operate(&call).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_operate_reduce_deviation_standard_float() {
        let input = &RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]);
        let call = (
            RadonOpCodes::ArrayReduce,
            Some(vec![Value::Integer(
                RadonReducers::DeviationStandard as i128,
            )]),
        );
        let expected = RadonTypes::from(RadonFloat::from(0.5));

        let output = input.operate(&call).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_operate_reduce_average_median() {
        let mut context = ReportContext {
            active_wips: Some(current_active_wips()),
            ..Default::default()
        };
        let input = &RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
            RadonFloat::from(2f64).into(),
        ]);
        let call = (
            RadonOpCodes::ArrayReduce,
            Some(vec![Value::Integer(RadonReducers::AverageMedian as i128)]),
        );

        let expected = RadonTypes::from(RadonFloat::from(2f64));
        let output = input.operate_in_context(&call, &mut context).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_operate_reduce_mode_float() {
        let input = RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
            RadonFloat::from(2f64).into(),
        ]);
        let call = (
            RadonOpCodes::ArrayReduce,
            Some(vec![Value::Integer(RadonReducers::Mode as i128)]),
        );
        let expected = RadonTypes::from(RadonFloat::from(2f64));

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
                    Value::Integer(RadonOpCodes::FloatMultiply as i128),
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

        assert!(result.is_err());
    }

    #[test]
    fn test_serialize_radon_array() {
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

    #[test]
    fn test_is_homogeneous() {
        let string0 = RadonTypes::String(RadonString::from("Hello"));
        let string1 = RadonTypes::String(RadonString::from("World"));

        let array = RadonArray::from(vec![string0, string1]);
        assert!(array.is_homogeneous());

        let int0 = RadonTypes::Integer(RadonInteger::from(0));
        let int1 = RadonTypes::Integer(RadonInteger::from(1));

        let array = RadonArray::from(vec![int0, int1]);
        assert!(array.is_homogeneous());

        let float0 = RadonTypes::Float(RadonFloat::from(0.5));
        let float1 = RadonTypes::Float(RadonFloat::from(1.5));

        let array = RadonArray::from(vec![float0.clone(), float1]);
        assert!(array.is_homogeneous());

        let bool0 = RadonTypes::Boolean(RadonBoolean::from(true));
        let bool1 = RadonTypes::Boolean(RadonBoolean::from(false));

        let array = RadonArray::from(vec![bool0, bool1]);
        assert!(array.is_homogeneous());

        let map0 = RadonTypes::Map(RadonMap::from(BTreeMap::default()));
        let map1 = RadonTypes::Map(RadonMap::from(BTreeMap::default()));

        let array = RadonArray::from(vec![map0, map1]);
        assert!(array.is_homogeneous());

        let array0 = RadonTypes::Array(RadonArray::from(vec![RadonTypes::Integer(
            RadonInteger::from(0),
        )]));
        let array1 = RadonTypes::Array(RadonArray::from(vec![RadonTypes::Integer(
            RadonInteger::from(1),
        )]));

        let array = RadonArray::from(vec![array0, array1]);
        assert!(array.is_homogeneous());

        let bytes0 = RadonTypes::Bytes(RadonBytes::from(vec![0x01, 0x02, 0x03]));
        let bytes1 = RadonTypes::Bytes(RadonBytes::from(vec![0x11, 0x12, 0x13]));

        let array = RadonArray::from(vec![bytes0, bytes1.clone()]);
        assert!(array.is_homogeneous());

        let array = RadonArray::from(vec![float0, bytes1]);
        assert!(!array.is_homogeneous());
    }
}
