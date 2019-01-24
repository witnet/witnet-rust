use crate::error::*;
use crate::types::{float::RadonFloat, mixed::RadonMixed, RadonType};

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
    use rmpv::Value;

    let radon_float = RadonFloat::from(std::f64::consts::PI);
    let radon_mixed_error = RadonMixed::from(Value::from(String::from("Hello world!")));
    let radon_mixed = RadonMixed::from(Value::from(std::f64::consts::PI));
    let radon_error = RadError::new(
        RadErrorKind::EncodeDecode,
        String::from("Failed to encode a RadonFloat from RadonMixed"),
    );
    assert_eq!(radon_float, as_float(radon_mixed).unwrap());
    assert_eq!(radon_error, as_float(radon_mixed_error).unwrap_err());
}
