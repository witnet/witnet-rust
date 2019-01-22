use crate::error::*;
use crate::types::{float::RadonFloat, mixed::RadonMixed, RadonType};
use rmpv::{Utf8String, Value};

pub fn as_float(input: RadonMixed) -> Result<RadonFloat, RadError> {
    match input.value().as_f64() {
        Some(value) => Ok(RadonFloat::from(value)),
        None => Err(RadError::new(
            RadErrorKind::EncodeDecode,
            String::from("Failed to encode a RadonFloat from RadonMixed"),
        )),
    }
}

#[test]
fn test_as_float() {
    let radon_float = RadonFloat::from(3.1415);
    let radon_mixed_error = RadonMixed::from(Value::String(Utf8String::from(String::from("asdf"))));
    let radon_mixed = RadonMixed::from(Value::F64(3.1415));
    let radon_error = RadError::new(
        RadErrorKind::EncodeDecode,
        String::from("Failed to encode a RadonFloat from RadonMixed"),
    );
    assert_eq!(radon_float, as_float(radon_mixed).unwrap());
    assert_eq!(radon_error, as_float(radon_mixed_error).unwrap_err());
}
