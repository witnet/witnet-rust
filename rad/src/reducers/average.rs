use crate::{
    error::RadError,
    operators::array::transpose,
    reducers::RadonReducers,
    types::{array::RadonArray, float::RadonFloat, RadonType, RadonTypes},
};
use std::ops::Div;

pub fn mean(input: &RadonArray) -> Result<RadonTypes, RadError> {
    let value = input.value();
    let value_len = value.len();

    match value.first() {
        None => Ok(RadonTypes::from(RadonFloat::from(std::f64::NAN))),
        Some(RadonTypes::Float(_)) => {
            let sum = value.iter().try_fold(0f64, |sum, item| match item {
                RadonTypes::Float(f64_value) => Ok(sum + f64_value.value()),
                _ => Err(RadError::MismatchingTypes {
                    method: RadonReducers::AverageMean.to_string(),
                    expected: RadonFloat::radon_type_name(),
                    found: item.clone().radon_type_name(),
                }),
            });
            let sum = sum?;

            // Divide sum by the count of numeric values that were summed
            let mean_value: f64 = sum.div(value_len as f64);

            Ok(RadonTypes::from(RadonFloat::from(mean_value)))
        }
        Some(RadonTypes::Array(_)) => {
            let v = transpose(input)?;

            let mut mean_v = vec![];
            for v2mean in v {
                mean_v.push(mean(&v2mean)?);
            }

            Ok(RadonTypes::from(RadonArray::from(mean_v)))
        }
        Some(rad_types) => Err(RadError::UnsupportedReducer {
            inner_type: rad_types.clone().radon_type_name(),
            reducer: RadonReducers::AverageMean.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operators::array::reduce;
    use serde_cbor::Value;

    #[test]
    fn test_reduce_average_mean_float() {
        use crate::types::float::RadonFloat;

        let input = &RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]);
        let args = &[Value::Integer(0x03)]; // This is RadonReducers::AverageMean
        let expected = RadonTypes::from(RadonFloat::from(1.5f64));

        let output = reduce(input, args).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_reduce_average_mean_float_arrays() {
        use crate::types::float::RadonFloat;

        let array_1 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]));
        let array_2 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(6f64).into(),
            RadonFloat::from(10f64).into(),
        ]));
        let input = RadonArray::from(vec![array_1, array_2]);

        let expected = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(3.5f64).into(),
            RadonFloat::from(6f64).into(),
        ]));

        let args = &[Value::Integer(0x03)]; // This is RadonReducers::AverageMean
        let output = reduce(&input, args).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_reduce_average_mean_float_arrays_different_size() {
        use crate::types::float::RadonFloat;

        let array_1 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
            RadonFloat::from(3f64).into(),
        ]));
        let array_2 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(6f64).into(),
            RadonFloat::from(10f64).into(),
        ]));
        let input = RadonArray::from(vec![array_1, array_2]);

        let expected = RadError::DifferentSizeArrays {
            method: "RadonArray::transpose".to_string(),
            first: 3,
            second: 2,
        };

        let args = &[Value::Integer(0x03)]; // This is RadonReducers::AverageMean
        let output = reduce(&input, args).unwrap_err();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_reduce_average_mean_float_array_of_arrays() {
        use crate::types::float::RadonFloat;

        let array_11 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]));
        let array_12 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(3f64).into(),
            RadonFloat::from(4f64).into(),
        ]));
        let array_13 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(5f64).into(),
            RadonFloat::from(6f64).into(),
        ]));
        let array1 = RadonTypes::from(RadonArray::from(vec![array_11, array_12, array_13]));

        let array_21 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(11f64).into(),
            RadonFloat::from(12f64).into(),
        ]));
        let array_22 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(13f64).into(),
            RadonFloat::from(14f64).into(),
        ]));
        let array_23 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(15f64).into(),
            RadonFloat::from(16f64).into(),
        ]));
        let array2 = RadonTypes::from(RadonArray::from(vec![array_21, array_22, array_23]));
        let input = RadonArray::from(vec![array1, array2]);

        let array_e1 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(6f64).into(),
            RadonFloat::from(7f64).into(),
        ]));
        let array_e2 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(8f64).into(),
            RadonFloat::from(9f64).into(),
        ]));
        let array_e3 = RadonTypes::from(RadonArray::from(vec![
            RadonFloat::from(10f64).into(),
            RadonFloat::from(11f64).into(),
        ]));
        let expected = RadonTypes::from(RadonArray::from(vec![array_e1, array_e2, array_e3]));

        let args = &[Value::Integer(0x03)]; // This is RadonReducers::AverageMean
        let output = reduce(&input, args).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn test_operate_reduce_mean_empty() {
        let input = RadonArray::from(vec![]);
        let output = mean(&input).unwrap();
        assert_eq!(output, RadonTypes::from(RadonFloat::from(std::f64::NAN)));
    }

    #[test]
    // TODO Remove this test after integer mean implementation
    fn test_reduce_average_mean_integer_unsupported() {
        use crate::types::integer::RadonInteger;

        let input = &RadonArray::from(vec![
            RadonInteger::from(1i128).into(),
            RadonInteger::from(2i128).into(),
        ]);
        let args = &[Value::Integer(0x03)]; // This is RadonReducers::AverageMean
        let output = reduce(input, args).unwrap_err();

        let expected = RadError::UnsupportedReducer {
            inner_type: "RadonInteger".to_string(),
            reducer: "RadonReducers::AverageMean".to_string(),
        };

        assert_eq!(output, expected);
    }
}
