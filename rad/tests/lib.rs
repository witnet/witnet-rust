use rmpv::Value;
use witnet_data_structures::serializers::decoders::TryFrom;
use witnet_rad::types::RadonTypes;

#[test]
fn test_radon_types_names() {
    let radon_array = RadonTypes::try_from(Value::from(vec![Value::from(123)])).unwrap();
    let radon_array_type_name = radon_array.radon_type_name();
    assert_eq!(radon_array_type_name, String::from("RadonArray"));

    let radon_float = RadonTypes::try_from(Value::from(std::f64::consts::PI)).unwrap();
    let radon_float_type_name = radon_float.radon_type_name();
    assert_eq!(radon_float_type_name, String::from("RadonFloat"));

    let radon_map = RadonTypes::try_from(Value::from(vec![(
        Value::from("Hello"),
        Value::from("World"),
    )]))
    .unwrap();
    let radon_map_type_name = radon_map.radon_type_name();
    assert_eq!(radon_map_type_name, String::from("RadonMap"));

    let radon_mixed = RadonTypes::try_from(Value::Ext(123, vec![1, 2, 3])).unwrap();
    let radon_mixed_type_name = radon_mixed.radon_type_name();
    assert_eq!(radon_mixed_type_name, String::from("RadonMixed"));

    let radon_string = RadonTypes::try_from(Value::from("Hello, World!")).unwrap();
    let radon_string_type_name = radon_string.radon_type_name();
    assert_eq!(radon_string_type_name, String::from("RadonString"));
}

#[test]
fn test_radon_types_display() {
    let radon_array = RadonTypes::try_from(Value::from(vec![Value::from(123)])).unwrap();
    let radon_array_type_display = radon_array.to_string();
    let radon_array_expected =
        "RadonTypes::RadonArray([Mixed(RadonMixed { value: Integer(PosInt(123)) })])".to_string();
    assert_eq!(radon_array_type_display, radon_array_expected);

    let radon_float = RadonTypes::try_from(Value::from(std::f64::consts::PI)).unwrap();
    let radon_float_type_display = radon_float.to_string();
    let radon_float_expected = "RadonTypes::RadonFloat(3.141592653589793)".to_string();
    assert_eq!(radon_float_type_display, radon_float_expected);

    let radon_map = RadonTypes::try_from(Value::from(vec![(
        Value::from("Hello"),
        Value::from("World"),
    )]))
    .unwrap();
    let radon_map_type_display = radon_map.to_string();
    let radon_map_expected = r#"RadonTypes::RadonMap({"Hello": RadonMixed { value: String(Utf8String { s: Ok("World") }) }})"#.to_string();
    assert_eq!(radon_map_type_display, radon_map_expected);

    let radon_mixed = RadonTypes::try_from(Value::Ext(123, vec![1, 2, 3])).unwrap();
    let radon_mixed_type_display = radon_mixed.to_string();
    let radon_mixed_expected = "RadonTypes::RadonMixed(Ext(123, [1, 2, 3]))".to_string();
    assert_eq!(radon_mixed_type_display, radon_mixed_expected);

    let radon_string = RadonTypes::try_from(Value::from("Hello, World!")).unwrap();
    let radon_string_type_display = radon_string.to_string();
    let radon_string_expected = r#"RadonTypes::RadonString("Hello, World!")"#.to_string();
    assert_eq!(radon_string_type_display, radon_string_expected);
}
