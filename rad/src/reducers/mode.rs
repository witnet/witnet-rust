use crate::error::RadError;
use crate::types::{array::RadonArray, RadonType, RadonTypes};
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

#[test]
fn test_operate_reduce_mode_float() {
    use crate::types::float::RadonFloat;

    let input = RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let _expected = RadonTypes::from(RadonFloat::from(2f64));
    let output = mode(&input).unwrap();
    assert_eq!(output, _expected);
}

#[test]
fn test_operate_reduce_mode_float_invalid() {
    use crate::types::float::RadonFloat;

    let input = RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);

    let output = mode(&input).unwrap_err();

    assert_eq!(output.to_string(), "There was a tie after applying the mode reducer on values: `RadonArray { value: [Float(RadonFloat { value: 1.0 }), Float(RadonFloat { value: 2.0 })], inner_type: Discriminant(2) }`".to_string());
}

#[test]
fn test_operate_reduce_mode_int() {
    use crate::types::integer::RadonInteger;

    let input = RadonArray::from(vec![
        RadonInteger::from(1i128).into(),
        RadonInteger::from(2i128).into(),
        RadonInteger::from(2i128).into(),
    ]);
    let _expected = RadonTypes::from(RadonInteger::from(2i128));
    let output = mode(&input).unwrap();
    assert_eq!(output, _expected);
}

#[test]
fn test_operate_reduce_mode_int_invalid() {
    use crate::types::integer::RadonInteger;

    let input = RadonArray::from(vec![
        RadonInteger::from(1i128).into(),
        RadonInteger::from(2i128).into(),
    ]);
    let output = mode(&input).unwrap_err();
    assert_eq!(output.to_string(), "There was a tie after applying the mode reducer on values: `RadonArray { value: [Integer(RadonInteger { value: 1 }), Integer(RadonInteger { value: 2 })], inner_type: Discriminant(6) }`".to_string());
}

#[test]
fn test_operate_reduce_mode_str() {
    use crate::types::string::RadonString;

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
    use crate::types::string::RadonString;

    let input = RadonArray::from(vec![
        RadonString::from("Hello world!").into(),
        RadonString::from("Bye world!").into(),
    ]);
    let output = mode(&input).unwrap_err();
    assert_eq!(output.to_string(), "There was a tie after applying the mode reducer on values: `RadonArray { value: [String(RadonString { value: \"Hello world!\" }), String(RadonString { value: \"Bye world!\" })], inner_type: Discriminant(5) }`");
}

#[test]
fn test_operate_reduce_mode_empty() {
    let input = RadonArray::from(vec![]);
    let output = mode(&input).unwrap_err();
    assert_eq!(
        output.to_string(),
        "Tried to apply mode reducer on an empty array"
    );
}
