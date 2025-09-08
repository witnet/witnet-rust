use base64::Engine;
use serde_cbor::value::{Value, from_value};
use std::convert::TryFrom;

use crate::{
    error::RadError,
    hash_functions::{self, RadonHashFunctions},
    types::{
        RadonType,
        bytes::{RadonBytes, RadonBytesEncoding},
        integer::RadonInteger,
        string::RadonString,
    },
};

pub fn as_integer(input: &RadonBytes) -> Result<RadonInteger, RadError> {
    let input_value_len = input.value().len();
    match input_value_len {
        1..=16 => {
            let mut bytes_array = [0u8; 16];
            bytes_array[16 - input_value_len..].copy_from_slice(&input.value());
            Ok(RadonInteger::from(i128::from_be_bytes(bytes_array)))
        }
        17.. => Err(RadError::ParseInt {
            message: "Input buffer too big".to_string(),
        }),
        _ => Err(RadError::EmptyArray),
    }
}

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
    let end_index = input.value().len();
    if end_index > 0 {
        let start_index = from_value::<i64>(args[0].clone())
            .unwrap_or_default()
            .rem_euclid(end_index as i64) as usize;
        let mut slice = input.value().as_slice().split_at(start_index).1.to_vec();
        if args.len() == 2 {
            let end_index = from_value::<i64>(args[1].clone())
                .unwrap_or_default()
                .rem_euclid(end_index as i64) as usize;
            slice.truncate(end_index - start_index);
        }
        Ok(RadonBytes::from(slice))
    } else {
        Err(wrong_args())
    }
}

pub fn to_string(input: &RadonBytes, args: &Option<Vec<Value>>) -> Result<RadonString, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: RadonString::radon_type_name(),
        operator: "Stringify".to_string(),
        args: args.to_owned().unwrap_or_default().to_vec(),
    };
    let mut bytes_encoding = RadonBytesEncoding::Hex;
    if let Some(args) = args {
        if !args.is_empty() {
            let arg = args.first().ok_or_else(wrong_args)?.to_owned();
            let bytes_encoding_u8 = from_value::<u8>(arg).map_err(|_| wrong_args())?;
            bytes_encoding =
                RadonBytesEncoding::try_from(bytes_encoding_u8).map_err(|_| wrong_args())?;
        }
    }
    match bytes_encoding {
        RadonBytesEncoding::Hex => RadonString::try_from(Value::Text(hex::encode(input.value()))),
        RadonBytesEncoding::Base58 => RadonString::try_from(Value::Text(bs58::encode(input.value()).into_string())),
        RadonBytesEncoding::Base64 => RadonString::try_from(Value::Text(
            base64::engine::general_purpose::STANDARD.encode(input.value()),
        )),
        RadonBytesEncoding::Utf8 => Ok(RadonString::from(
            String::from_utf8(input.value().to_vec()).unwrap_or_default(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes_to_string() {
        let input = RadonBytes::from(vec![0x01, 0x02, 0x03]);
        let valid_args = Some(vec![Value::from(0x00)]);
        let output = to_string(&input, &valid_args).unwrap().value();

        let valid_expected = "010203".to_string();

        assert_eq!(output, valid_expected);
    }

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
}
