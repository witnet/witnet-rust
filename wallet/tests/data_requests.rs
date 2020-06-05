use std::collections::HashMap;

use witnet_data_structures::chain::RADType;
use witnet_data_structures::{
    chain::{RADAggregate, RADRequest, RADRetrieve, RADTally},
    radon_error::RadonError,
};
use witnet_rad::script::unpack_radon_script;
use witnet_rad::{
    error::RadError,
    script::RadonScriptExecutionSettings,
    try_data_request,
    types::{
        array::RadonArray, boolean::RadonBoolean, bytes::RadonBytes, float::RadonFloat,
        integer::RadonInteger, map::RadonMap, string::RadonString, RadonTypes,
    },
};

#[test]
fn test_radon_types_json_serialization() {
    let radon_type = RadonTypes::from(RadonArray::from(vec![
        RadonTypes::from(RadonString::from("foo")),
        RadonTypes::from(RadonFloat::from(std::f64::consts::PI)),
    ]));
    let expected_json =
        r#"{"RadonArray":[{"RadonString":"foo"},{"RadonFloat":3.141592653589793}]}"#;
    assert_eq!(serde_json::to_string(&radon_type).unwrap(), expected_json);

    let radon_type = RadonTypes::from(RadonBoolean::from(true));
    let expected_json = r#"{"RadonBoolean":true}"#;
    assert_eq!(serde_json::to_string(&radon_type).unwrap(), expected_json);

    let radon_type = RadonTypes::from(RadonBytes::from(vec![1, 2, 3]));
    let expected_json = r#"{"RadonBytes":[1,2,3]}"#;
    assert_eq!(serde_json::to_string(&radon_type).unwrap(), expected_json);

    let radon_type = RadonTypes::from(RadonFloat::from(std::f64::consts::PI));
    let expected_json = r#"{"RadonFloat":3.141592653589793}"#;
    assert_eq!(serde_json::to_string(&radon_type).unwrap(), expected_json);

    let radon_type = RadonTypes::from(RadonInteger::from(42));
    let expected_json = r#"{"RadonInteger":42}"#;
    assert_eq!(serde_json::to_string(&radon_type).unwrap(), expected_json);

    let radon_type = RadonTypes::from(RadonMap::from(
        vec![(
            String::from("foo"),
            RadonTypes::from(RadonString::from("bar")),
        )]
        .iter()
        .cloned()
        .collect::<HashMap<String, RadonTypes>>(),
    ));
    let expected_json = r#"{"RadonMap":{"foo":{"RadonString":"bar"}}}"#;
    assert_eq!(serde_json::to_string(&radon_type).unwrap(), expected_json);

    let radon_type = RadonTypes::from(RadonString::from("foo"));
    let expected_json = r#"{"RadonString":"foo"}"#;
    assert_eq!(serde_json::to_string(&radon_type).unwrap(), expected_json);
}

#[test]
fn test_radon_error_json_serialization() {
    let radon_error = RadonTypes::RadonError(RadonError::new(RadError::default()));
    let expected_json = r#"{"RadonError":"Unknown error"}"#;
    assert_eq!(serde_json::to_string(&radon_error).unwrap(), expected_json);
}

#[test]
/// This is a rather end-2-end test that applies a script on some JSON input and checks whether the
/// final `RadonReport` complies with the Witnet Wallet API.
fn test_data_request_report_json_serialization() {
    let input = RadonTypes::from(RadonString::from(
        r#"{"coord":{"lon":13.41,"lat":52.52},"weather":[{"id":600,"main":"Snow","description":"light snow","icon":"13n"}],"base":"stations","main":{"temp":-4,"pressure":1013,"humidity":73,"temp_min":-4,"temp_max":-4},"visibility":10000,"wind":{"speed":2.6,"deg":90},"clouds":{"all":75},"dt":1548346800,"sys":{"type":1,"id":1275,"message":0.0038,"country":"DE","sunrise":1548313160,"sunset":1548344298},"id":2950159,"name":"Berlin","cod":200}"#,
    ));
    let script = vec![
        (RadonOpCodes::StringParseJSONMap, None),
        (
            RadonOpCodes::MapGetMap,
            Some(vec![Value::Text(String::from("main"))]),
        ),
        (
            RadonOpCodes::MapGetFloat,
            Some(vec![Value::Text(String::from("temp"))]),
        ),
    ];
    let mut context = ReportContext::default();
    let report = execute_radon_script(
        input,
        &script,
        &mut context,
        RadonScriptExecutionSettings::enable_all(),
    )
    .unwrap();

    let json = serde_json::ser::to_string(&report).unwrap();

    assert_eq!(json, "");
}
