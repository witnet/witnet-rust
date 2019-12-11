use serde_cbor::value::{from_value, Value};
use std::convert::TryFrom;

use crate::{
    error::RadError,
    hash_functions::{self, RadonHashFunctions},
    types::{bytes::RadonBytes, string::RadonString, RadonType},
};

pub fn to_string(input: &RadonBytes) -> Result<RadonString, RadError> {
    RadonString::try_from(Value::Text(hex::encode(input.value())))
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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes_to_string() {
        let input = RadonBytes::from(vec![0x01, 0x02, 0x03]);
        let output = to_string(&input).unwrap().value();

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
