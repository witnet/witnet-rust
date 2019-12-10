use serde_cbor::value::Value;
use std::collections::btree_map::BTreeMap;
use std::convert::TryFrom;
use witnet_rad::types::RadonTypes;

#[test]
fn test_radon_types_names() {
    let radon_array = RadonTypes::try_from(Value::Array(vec![Value::Integer(123)])).unwrap();
    let radon_array_type_name = radon_array.radon_type_name();
    assert_eq!(radon_array_type_name, String::from("RadonArray"));

    let radon_float = RadonTypes::try_from(Value::Float(std::f64::consts::PI)).unwrap();
    let radon_float_type_name = radon_float.radon_type_name();
    assert_eq!(radon_float_type_name, String::from("RadonFloat"));

    let radon_map = RadonTypes::try_from(Value::Map(BTreeMap::new())).unwrap();
    let radon_map_type_name = radon_map.radon_type_name();
    assert_eq!(radon_map_type_name, String::from("RadonMap"));

    let radon_bytes = RadonTypes::try_from(Value::Bytes(vec![1, 2, 3])).unwrap();
    let radon_bytes_type_name = radon_bytes.radon_type_name();
    assert_eq!(radon_bytes_type_name, String::from("RadonMixed"));

    let radon_string = RadonTypes::try_from(Value::Text(String::from("Hello, World!"))).unwrap();
    let radon_string_type_name = radon_string.radon_type_name();
    assert_eq!(radon_string_type_name, String::from("RadonString"));
}

#[test]
fn test_radon_types_display() {
    let radon_array = RadonTypes::try_from(Value::Array(vec![Value::Integer(123)])).unwrap();
    let radon_array_type_display = radon_array.to_string();
    let radon_array_expected =
        "RadonTypes::RadonArray([Integer(RadonInteger { value: 123 })])".to_string();
    assert_eq!(radon_array_type_display, radon_array_expected);

    let radon_float = RadonTypes::try_from(Value::Float(std::f64::consts::PI)).unwrap();
    let radon_float_type_display = radon_float.to_string();
    let radon_float_expected = "RadonTypes::RadonFloat(3.141592653589793)".to_string();
    assert_eq!(radon_float_type_display, radon_float_expected);

    let mut map = BTreeMap::new();
    map.insert(
        Value::Text(String::from("Hello")),
        Value::Text(String::from("World")),
    );
    let radon_map = RadonTypes::try_from(Value::Map(map)).unwrap();
    let radon_map_type_display = radon_map.to_string();
    let radon_map_expected =
        r#"RadonTypes::RadonMap({"Hello": RadonMixed { value: Text("World") }})"#.to_string();
    assert_eq!(radon_map_type_display, radon_map_expected);

    let radon_bytes = RadonTypes::try_from(Value::Bytes(vec![1, 2, 3])).unwrap();
    let radon_bytes_type_display = radon_bytes.to_string();
    let radon_bytes_expected = "RadonTypes::RadonMixed(Bytes([1, 2, 3]))".to_string();
    assert_eq!(radon_bytes_type_display, radon_bytes_expected);

    let radon_string = RadonTypes::try_from(Value::Text(String::from("Hello, World!"))).unwrap();
    let radon_string_type_display = radon_string.to_string();
    let radon_string_expected = r#"RadonTypes::RadonString("Hello, World!")"#.to_string();
    assert_eq!(radon_string_type_display, radon_string_expected);
}
