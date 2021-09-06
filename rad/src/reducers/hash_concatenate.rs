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
            let concatenated_bytes =
                value
                    .iter()
                    .try_fold(vec![], |mut bytes, item| match item {
                        RadonTypes::Bytes(rad_bytes) => {
                            let new_bytes = rad_bytes.value();
                            match new_bytes.len() {
                                32 => bytes.extend_from_slice(&new_bytes),
                                x if x < 32 => {
                                    let diff = 32 - x;
                                    bytes.extend_from_slice(&new_bytes);
                                    for _i in 0..diff {
                                        bytes.extend_from_slice(&[0]);
                                    }
                                }
                                x if x > 32 => {
                                    bytes.extend_from_slice(&new_bytes[..32]);
                                }
                                _ => {
                                    unreachable!()
                                }
                            }

                            Ok(bytes)
                        }
                        _ => Err(RadError::MismatchingTypes {
                            method: RadonReducers::HashConcatenate.to_string(),
                            expected: RadonBytes::radon_type_name(),
                            found: item.clone().radon_type_name(),
                        }),
                    })?;

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
