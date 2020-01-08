use crate::{
    error::RadError,
    types::{array::RadonArray, RadonType, RadonTypes},
};
use std::collections::HashMap;

pub fn mode(input: &RadonArray) -> Result<RadonTypes, RadError> {
    let value = input.value();

    let mut counter: HashMap<RadonTypes, i8> = HashMap::new();

    // Count how many times does each different item appear in the input array
    for item in value {
        *counter.entry(item).or_insert(0) += 1;
    }

    let temp_counter = counter.clone();

    // Compute how many times does the most frequent item appear
    let max_count = temp_counter
        .values()
        .max()
        .ok_or_else(|| RadError::ModeEmpty)?;

    // Collect items that appear as many times as the one that appears the most
    let mode_vector: Vec<RadonTypes> = counter
        .into_iter()
        .filter(|&(_, v)| &v == max_count)
        .map(|(k, _)| k)
        .collect();

    // Returns the mode or an error if there is a tie
    if mode_vector.len() > 1 {
        Err(RadError::ModeTie {
            values: input.clone(),
        })
    } else {
        Ok(mode_vector[0].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        error::RadError::{ModeEmpty, ModeTie},
        types::{float::RadonFloat, integer::RadonInteger, string::RadonString},
    };

    #[test]
    fn test_operate_reduce_mode_float() {
        let input = RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
            RadonFloat::from(2f64).into(),
        ]);
        let expected = RadonTypes::from(RadonFloat::from(2f64));
        let output = mode(&input).unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_operate_reduce_mode_float_invalid() {
        let input = RadonArray::from(vec![
            RadonFloat::from(1f64).into(),
            RadonFloat::from(2f64).into(),
        ]);

        let output = mode(&input).unwrap_err();
        let expected_error = ModeTie { values: input };
        assert_eq!(output, expected_error);
    }

    #[test]
    fn test_operate_reduce_mode_int() {
        let input = RadonArray::from(vec![
            RadonInteger::from(1i128).into(),
            RadonInteger::from(2i128).into(),
            RadonInteger::from(2i128).into(),
        ]);
        let expected = RadonTypes::from(RadonInteger::from(2i128));
        let output = mode(&input).unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_operate_reduce_mode_int_invalid() {
        let input = RadonArray::from(vec![
            RadonInteger::from(1i128).into(),
            RadonInteger::from(2i128).into(),
        ]);
        let output = mode(&input).unwrap_err();
        let expected_error = ModeTie { values: input };
        assert_eq!(output, expected_error);
    }

    #[test]
    fn test_operate_reduce_mode_str() {
        let input = RadonArray::from(vec![
            RadonString::from("Hello world!").into(),
            RadonString::from("Hello world!").into(),
            RadonString::from("Bye world!").into(),
        ]);
        let expected = RadonString::from("Hello world!").into();
        let output = mode(&input).unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_operate_reduce_mode_str_invalid() {
        let input = RadonArray::from(vec![
            RadonString::from("Hello world!").into(),
            RadonString::from("Bye world!").into(),
        ]);
        let output = mode(&input).unwrap_err();
        let expected_error = ModeTie { values: input };
        assert_eq!(output, expected_error);
    }

    #[test]
    fn test_operate_reduce_mode_empty() {
        let input = RadonArray::from(vec![]);
        let output = mode(&input).unwrap_err();
        let expected_error = ModeEmpty;
        assert_eq!(output, expected_error);
    }

    #[test]
    fn test_operate_reduce_mode_array() {
        let array1 = RadonArray::from(vec![
            RadonInteger::from(1i128).into(),
            RadonInteger::from(2i128).into(),
            RadonInteger::from(3i128).into(),
        ]);
        let array2 = RadonArray::from(vec![
            RadonInteger::from(2i128).into(),
            RadonInteger::from(5i128).into(),
            RadonInteger::from(4i128).into(),
        ]);
        let array3 = RadonArray::from(vec![
            RadonInteger::from(1i128).into(),
            RadonInteger::from(2i128).into(),
            RadonInteger::from(3i128).into(),
            RadonInteger::from(4i128).into(),
        ]);

        let input = RadonArray::from(vec![
            array1.clone().into(),
            array1.clone().into(),
            array1.clone().into(),
            array2.into(),
            array3.into(),
        ]);

        let expected = RadonTypes::from(array1);
        let output = mode(&input).unwrap();
        assert_eq!(output, expected);
    }
}
