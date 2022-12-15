use crate::{
    error::RadError,
    hash_functions::RadonHashFunctions,
    operators::bytes as bytes_operators,
    reducers::RadonReducers,
    types::{array::RadonArray, bytes::RadonBytes, RadonType, RadonTypes},
};
use serde_cbor::value::Value;

/// Reducer that concatenate all the RadonTypes and hash
pub fn hash_concatenate(input: &RadonArray) -> Result<RadonTypes, RadError> {
    let value = input.value();

    match value.first() {
        None => Err(RadError::UnsupportedReducer {
            array: input.clone(),
            reducer: RadonReducers::HashConcatenate.to_string(),
        }),
        Some(RadonTypes::Bytes(_)) => {
            let concatenated_bytes = value.iter().try_fold(
                Vec::with_capacity(32 * value.len()),
                |mut bytes, item| match item {
                    RadonTypes::Bytes(rad_bytes) => {
                        let new_bytes = rad_bytes.value();

                        // Take up to 32 bytes from new_bytes
                        let new_bytes_len = std::cmp::min(new_bytes.len(), 32);
                        // In case of less than 32 bytes, it will be zero-padded
                        for _i in new_bytes_len..32 {
                            bytes.push(0);
                        }
                        bytes.extend_from_slice(&new_bytes[..new_bytes_len]);

                        Ok(bytes)
                    }
                    _ => Err(RadError::MismatchingTypes {
                        method: RadonReducers::HashConcatenate.to_string(),
                        expected: RadonBytes::radon_type_name(),
                        found: item.clone().radon_type_name(),
                    }),
                },
            )?;

            let hash_function = [Value::from(u8::from(RadonHashFunctions::SHA2_256))];
            let radon_bytes =
                bytes_operators::hash(&RadonBytes::from(concatenated_bytes), &hash_function)
                    .unwrap();

            Ok(RadonTypes::from(radon_bytes))
        }
        Some(_rad_types) => Err(RadError::UnsupportedReducer {
            array: input.clone(),
            reducer: RadonReducers::HashConcatenate.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::types::integer::RadonInteger;
    use witnet_crypto::hash::calculate_sha256;

    #[test]
    fn hash_concat_empty_array() {
        let input = RadonArray::from(vec![]);
        let err = hash_concatenate(&input).unwrap_err();
        assert_eq!(
            err,
            RadError::UnsupportedReducer {
                array: input,
                reducer: "RadonReducers::HashConcatenate".to_string()
            }
        )
    }

    #[test]
    fn hash_concat_not_bytes() {
        let input = RadonArray::from(vec![RadonTypes::from(RadonInteger::from(1))]);
        let err = hash_concatenate(&input).unwrap_err();
        assert_eq!(
            err,
            RadError::UnsupportedReducer {
                array: input,
                reducer: "RadonReducers::HashConcatenate".to_string()
            }
        )
    }

    #[test]
    fn hash_concat_1_elem_0_bytes() {
        // Empty input is zero-padded to 32 bytes
        let input = RadonArray::from(vec![RadonTypes::from(RadonBytes::from(vec![]))]);
        let res = hash_concatenate(&input).unwrap();
        let expected = RadonTypes::from(RadonBytes::from(
            calculate_sha256(
                &hex::decode("0000000000000000000000000000000000000000000000000000000000000000")
                    .unwrap(),
            )
            .as_ref()
            .to_vec(),
        ));
        assert_eq!(res, expected);
    }

    #[test]
    fn hash_concat_1_elem_1_byte() {
        // 1 byte input is zero-padded to 32 bytes
        let input = RadonArray::from(vec![RadonTypes::from(RadonBytes::from(
            hex::decode("6b").unwrap(),
        ))]);
        let res = hash_concatenate(&input).unwrap();
        let expected = RadonTypes::from(RadonBytes::from(
            calculate_sha256(
                &hex::decode("000000000000000000000000000000000000000000000000000000000000006b")
                    .unwrap(),
            )
            .as_ref()
            .to_vec(),
        ));
        assert_eq!(res, expected);
    }

    #[test]
    fn hash_concat_1_elem_32_bytes() {
        let input = RadonArray::from(vec![RadonTypes::from(RadonBytes::from(
            hex::decode("6b86b273ff34fce19d6b804eff5a3f5747ada4eaa22f1d49c01e52ddb7875b4b")
                .unwrap(),
        ))]);
        let res = hash_concatenate(&input).unwrap();
        let expected = RadonTypes::from(RadonBytes::from(
            calculate_sha256(
                &hex::decode("6b86b273ff34fce19d6b804eff5a3f5747ada4eaa22f1d49c01e52ddb7875b4b")
                    .unwrap(),
            )
            .as_ref()
            .to_vec(),
        ));
        assert_eq!(res, expected);
    }

    #[test]
    fn hash_concat_1_elem_33_bytes() {
        // Input with more than 32 bytes is truncated to 32 bytes
        let input = RadonArray::from(vec![RadonTypes::from(RadonBytes::from(
            hex::decode("6b86b273ff34fce19d6b804eff5a3f5747ada4eaa22f1d49c01e52ddb7875b4bff")
                .unwrap(),
        ))]);
        let res = hash_concatenate(&input).unwrap();
        let expected = RadonTypes::from(RadonBytes::from(
            calculate_sha256(
                &hex::decode("6b86b273ff34fce19d6b804eff5a3f5747ada4eaa22f1d49c01e52ddb7875b4b")
                    .unwrap(),
            )
            .as_ref()
            .to_vec(),
        ));
        assert_eq!(res, expected);
    }

    #[test]
    fn hash_concat_4_elem_variable_bytes() {
        let input = RadonArray::from(vec![
            RadonTypes::from(RadonBytes::from(hex::decode("").unwrap())),
            RadonTypes::from(RadonBytes::from(hex::decode("d4").unwrap())),
            RadonTypes::from(RadonBytes::from(
                hex::decode("4e07408562bedb8b60ce05c1decfe3ad16b72230967de01f640b7e4729b49fce")
                    .unwrap(),
            )),
            RadonTypes::from(RadonBytes::from(
                hex::decode(
                    "4b227777d4dd1fc61c6f884f48641d02b4d121d3fd328cb08b5531fcacdabf8affffff",
                )
                .unwrap(),
            )),
        ]);
        let res = hash_concatenate(&input).unwrap();
        let expected = RadonTypes::from(RadonBytes::from(
            calculate_sha256(
                &hex::decode(
                    [
                        "0000000000000000000000000000000000000000000000000000000000000000",
                        "00000000000000000000000000000000000000000000000000000000000000d4",
                        "4e07408562bedb8b60ce05c1decfe3ad16b72230967de01f640b7e4729b49fce",
                        "4b227777d4dd1fc61c6f884f48641d02b4d121d3fd328cb08b5531fcacdabf8a",
                    ]
                    .concat(),
                )
                .unwrap(),
            )
            .as_ref()
            .to_vec(),
        ));
        assert_eq!(res, expected);
    }
}
