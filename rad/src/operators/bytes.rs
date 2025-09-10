use base64::Engine;
use serde_cbor::value::{Value, from_value};
use std::convert::TryFrom;

use crate::{
    error::RadError,
    hash_functions::{self, RadonHashFunctions},
    types::{
        RadonType,
        bytes::{RadonBytes, RadonBytesEncoding, RadonBytesEndianness},
        integer::RadonInteger,
        string::RadonString,
    },
};

pub fn hash(input: &RadonBytes, args: &[Value]) -> Result<RadonBytes, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonBytes::radon_type_name(),
        operator: "Hash".to_string(),
        args: args.to_vec(),
    };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let hash_function_integer = from_value::<u8>(arg).map_err(|_| wrong_args())?;
    let hash_function_code =
        RadonHashFunctions::try_from(hash_function_integer).map_err(|_| wrong_args())?;

    let digest = hash_functions::hash(input.value().as_slice(), hash_function_code)?;

    Ok(RadonBytes::from(digest))
}

pub fn length(input: &RadonBytes) -> RadonInteger {
    RadonInteger::from(input.value().len() as i128)
}

pub fn slice(input: &RadonBytes, args: &[Value]) -> Result<RadonBytes, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonString::radon_type_name(),
        operator: "BytesSlice".to_string(),
        args: args.to_vec(),
    };

    let input_length = input.value().len() as i32;
    let mut start = args
        .first()
        .ok_or_else(wrong_args)
        .cloned()
        .and_then(|value| from_value::<i32>(value).map_err(|_| wrong_args()))?;
    let mut end = args
        .get(1)
        .ok_or_else(wrong_args)
        .cloned()
        .and_then(|value| from_value::<i32>(value).map_err(|_| wrong_args()))
        .unwrap_or(input_length);

    // Allow negative indexes (e.g. -1 standing for the last byte in the buffer) and enforce range
    // caps
    if start < 0 {
        start += input_length;
    }
    if end < 0 {
        end += input_length;
    }
    start = start.max(0).min(input_length);
    end = end.min(input_length).max(0);
    let output_length = (end as usize).saturating_sub(start as usize);

    // Only perform real slicing if the sliced range is not empty
    let sliced_bytes = if output_length > 0 {
        input
            .value()
            .as_slice()
            .split_at(start as usize)
            .1
            .split_at(output_length)
            .0
            .to_owned()
    } else {
        vec![]
    };

    Ok(RadonBytes::from(sliced_bytes))
}

pub fn to_integer(input: &RadonBytes, args: &Option<Vec<Value>>) -> Result<RadonInteger, RadError> {
    let endianness = decode_single_arg::<u8, RadonBytesEndianness, _>(args, "ToInteger")?;

    match input.value().len() {
        // There is nothing to decode if the input is empty
        0 => Err(RadError::EmptyArray),
        // Happy path (the bytes buffer contains between 1 and 16 bytes)
        input_value_len @ 1..=16 => {
            let mut bytes_array = [0u8; 16];
            bytes_array[16 - input_value_len..].copy_from_slice(&input.value());

            // Use different functions for turning the bytes into an integer, based on the specified
            // endianness
            let decoder = if endianness == RadonBytesEndianness::Big {
                i128::from_be_bytes
            } else {
                i128::from_le_bytes
            };

            Ok(RadonInteger::from(decoder(bytes_array)))
        }
        // We can only decode integers up to 128 bits / 16 bytes
        17.. => Err(RadError::ParseInt {
            message: "Input buffer too long (>16 bytes)".to_string(),
        }),
    }
}

pub fn to_string(input: &RadonBytes, args: &Option<Vec<Value>>) -> Result<RadonString, RadError> {
    let bytes_encoding = decode_single_arg::<u8, RadonBytesEncoding, _>(args, "ToString")?;

    match bytes_encoding {
        RadonBytesEncoding::Hex => RadonString::try_from(Value::Text(hex::encode(input.value()))),
        RadonBytesEncoding::Base64 => RadonString::try_from(Value::Text(
            base64::engine::general_purpose::STANDARD.encode(input.value()),
        )),
        RadonBytesEncoding::Utf8 => Ok(RadonString::from(
            String::from_utf8(input.value().to_vec()).unwrap_or_default(),
        )),
    }
}

fn decode_single_arg<T1, T2, TS>(args: &Option<Vec<Value>>, operator: TS) -> Result<T2, RadError>
where
    T1: serde::de::DeserializeOwned,
    T2: Default + TryFrom<T1>,
    TS: ToString,
{
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonBytes::radon_type_name(),
        operator: operator.to_string(),
        args: args.to_owned().unwrap_or_default().to_vec(),
    };

    // Fail on wrong arguments but not on receiving no arguments or empty arguments, in which case
    // we want to use the default value
    if let Some(value) = args.to_owned().and_then(|args| args.first().cloned()) {
        from_value::<T1>(value)
            .map_err(|_| wrong_args())
            .and_then(|t1| T2::try_from(t1).map_err(|_| wrong_args()))
    } else {
        Ok(T2::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::RadError::*;

    #[test]
    fn test_bytes_hash() {
        let input = RadonBytes::from(vec![0x01, 0x02, 0x03]);
        let valid_args = [Value::from(0x0A)]; // 0x0A is RadonHashFunctions::SHA_256
        let wrong_args = [Value::from(0xFE)]; // 0xFF is not a member of RadonHashFunctions
        let unsupported_args = [Value::from(0xFF)]; // -1 is RadonHashFunctions::Fail (unsupported)

        let valid_output = hash(&input, &valid_args).unwrap();
        let wrong_output = hash(&input, &wrong_args);
        let unsupported_output = hash(&input, &unsupported_args);

        let valid_expected = "039058c6f2c0cb492c533b0a4d14ef77cc0f78abccced5287d84a1a2011cfb81";

        assert_eq!(hex::encode(valid_output.value()), valid_expected);
        assert_eq!(
            &wrong_output.unwrap_err().to_string(),
            "Wrong `RadonBytes::Hash()` arguments: `[Integer(254)]`"
        );
        assert_eq!(
            &unsupported_output.unwrap_err().to_string(),
            "Hash function `RadonHashFunctions::Fail` is not implemented"
        );
    }

    #[test]
    fn test_bytes_length() {
        // Empty bytes buffer
        let input = RadonBytes::from(vec![]);
        let output = length(&input);
        let expected = RadonInteger::from(0);
        assert_eq!(output, expected);

        // Small bytes buffer
        let input = RadonBytes::from(vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        let output = length(&input);
        let expected = RadonInteger::from(6);
        assert_eq!(output, expected);

        // Humongous bytes buffer (1GB), let us hope it does not break GitHub Actions :lol:
        let input = RadonBytes::from(vec![0x00; 1_000_000_000]);
        let output = length(&input);
        let expected = RadonInteger::from(1_000_000_000);
        assert_eq!(output, expected);
    }

    #[test]
    fn test_bytes_slice() {
        let input = RadonBytes::from(vec![
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09,
        ]);

        // No arguments
        let args = Vec::<Value>::new();
        let output = slice(&input, &args);
        let expected = Err(WrongArguments {
            input_type: "RadonString",
            operator: "BytesSlice".to_string(),
            args,
        });
        assert_eq!(output, expected);

        // Only a positive in-range start index is specified
        let args = vec![Value::Integer(5)];
        let output = slice(&input, &args);
        let expected = Ok(RadonBytes::from(vec![0x05, 0x06, 0x07, 0x08, 0x09]));
        assert_eq!(output, expected);

        // Only a positive out-of-range start index is specified
        let args = vec![Value::Integer(15)];
        let output = slice(&input, &args);
        let expected = Ok(RadonBytes::from(vec![]));
        assert_eq!(output, expected);

        // Only a negative in-range start index is specified
        let args = vec![Value::Integer(-2)];
        let output = slice(&input, &args);
        let expected = Ok(RadonBytes::from(vec![0x08, 0x09]));
        assert_eq!(output, expected);

        // Only a negative out-of-range start index is specified
        let args = vec![Value::Integer(-15)];
        let output = slice(&input, &args);
        let expected = Ok(input.clone());
        assert_eq!(output, expected);

        // A positive in-range start index and a positive in-range index are specified
        let args = vec![Value::Integer(5), Value::Integer(7)];
        let output = slice(&input, &args);
        let expected = Ok(RadonBytes::from(vec![0x05, 0x06]));
        assert_eq!(output, expected);

        // A positive in-range start index and a negative in-range index are specified
        let args = vec![Value::Integer(5), Value::Integer(-3)];
        let output = slice(&input, &args);
        let expected = Ok(RadonBytes::from(vec![0x05, 0x06]));
        assert_eq!(output, expected);

        // A negative in-range start index and a negative in-range index are specified
        let args = vec![Value::Integer(-5), Value::Integer(-3)];
        let output = slice(&input, &args);
        let expected = Ok(RadonBytes::from(vec![0x05, 0x06]));
        assert_eq!(output, expected);

        // Everything is in range
        let args = vec![Value::Integer(-15), Value::Integer(15)];
        let output = slice(&input, &args);
        let expected = Ok(input.clone());
        assert_eq!(output, expected);

        // Nothing is in range
        let args = vec![Value::Integer(15), Value::Integer(-15)];
        let output = slice(&input, &args);
        let expected = Ok(RadonBytes::from(vec![]));
        assert_eq!(output, expected);
    }

    #[test]
    fn test_bytes_to_integer() {
        let input = RadonBytes::from(vec![0x01, 0x02, 0x03, 0x04]);

        // No arguments, default to big
        let args = None;
        let output = to_integer(&input, &args);
        let expected = Ok(RadonInteger::from(0x00000000000000000000000001020304));
        assert_eq!(output, expected);

        // Empty arguments, default to big
        let args = Some(vec![]);
        let output = to_integer(&input, &args);
        let expected = Ok(RadonInteger::from(0x00000000000000000000000001020304));
        assert_eq!(output, expected);

        // Big endian
        let args = Some(vec![Value::Integer(i128::from(
            RadonBytesEndianness::Big as u8,
        ))]);
        let output = to_integer(&input, &args);
        let expected = Ok(RadonInteger::from(0x00000000000000000000000001020304));
        assert_eq!(output, expected);

        // Little endian
        let args = Some(vec![Value::Integer(i128::from(
            RadonBytesEndianness::Little as u8,
        ))]);
        let output = to_integer(&input, &args);
        let expected = Ok(RadonInteger::from(0x04030201000000000000000000000000));
        assert_eq!(output, expected);

        // Any non-little is a big
        let args = Some(vec![Value::Integer(123)]);
        let output = to_integer(&input, &args);
        let expected = Ok(RadonInteger::from(0x00000000000000000000000001020304));
        assert_eq!(output, expected);

        // Invalid argument semantics, fail
        let args = Some(vec![Value::Integer(123456)]);
        let output = to_integer(&input, &args);
        let expected = Err(WrongArguments {
            input_type: "RadonBytes",
            operator: "ToInteger".to_string(),
            args: args.unwrap(),
        });
        assert_eq!(output, expected);

        // Invalid argument type, fail
        let args = Some(vec![Value::Text(String::from("whatever"))]);
        let output = to_integer(&input, &args);
        let expected = Err(WrongArguments {
            input_type: "RadonBytes",
            operator: "ToInteger".to_string(),
            args: args.unwrap(),
        });
        assert_eq!(output, expected);
    }

    #[test]
    fn test_bytes_to_string() {
        let input = RadonBytes::from(vec![0x01, 0x02, 0x03]);

        // No arguments, default to Hex
        let args = None;
        let output = to_string(&input, &args).unwrap().value();
        let expected = "010203".to_string();
        assert_eq!(output, expected);

        // Empty arguments, default to Hex
        let args = Some(vec![]);
        let output = to_string(&input, &args).unwrap().value();
        let expected = "010203".to_string();
        assert_eq!(output, expected);

        // Wrong arguments, fail
        let args = Some(vec![Value::Text(String::from("whatever"))]);
        let output = to_string(&input, &args).unwrap_err();
        let expected = WrongArguments {
            input_type: "RadonBytes",
            operator: "ToString".to_string(),
            args: args.unwrap(),
        };
        assert_eq!(output, expected);

        // Hex
        let args = Some(vec![Value::from(RadonBytesEncoding::Hex as u8)]);
        let output = to_string(&input, &args).unwrap().value();
        let expected = "010203".to_string();
        assert_eq!(output, expected);

        // Base64
        let args = Some(vec![Value::from(RadonBytesEncoding::Base64 as u8)]);
        let output = to_string(&input, &args).unwrap().value();
        let expected = "AQID".to_string();
        assert_eq!(output, expected);

        // Utf-8
        let args = Some(vec![Value::from(RadonBytesEncoding::Utf8 as u8)]);
        let output = to_string(&input, &args).unwrap().value();
        let expected = "\u{1}\u{2}\u{3}".to_string();
        assert_eq!(output, expected);
    }
}
