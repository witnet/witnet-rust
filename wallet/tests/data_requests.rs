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

/// This is a rather end-2-end test that applies a script on some JSON input and checks whether the
/// final `RadonReport` complies with the Witnet Wallet API.
#[actix_rt::test]
#[cfg(feature = "side_effected")]
async fn test_data_request_report_json_serialization() {
    let request = RADRequest {
        time_lock: 0,
        retrieve: vec![
            RADRetrieve {
                kind: RADType::HttpGet,
                url: String::from("https://www.bitstamp.net/api/ticker/"),
                script: vec![130, 24, 119, 130, 24, 100, 100, 108, 97, 115, 116],
            },
            RADRetrieve {
                kind: RADType::HttpGet,
                url: String::from("https://api.coindesk.com/v1/bpi/currentprice.json"),
                script: vec![
                    132, 24, 119, 130, 24, 102, 99, 98, 112, 105, 130, 24, 102, 99, 85, 83, 68,
                    130, 24, 100, 106, 114, 97, 116, 101, 95, 102, 108, 111, 97, 116,
                ],
            },
        ],
        aggregate: RADAggregate {
            filters: vec![],
            reducer: 3,
        },
        tally: RADTally {
            filters: vec![],
            reducer: 3,
        },
    };

    let report = try_data_request(&request, RadonScriptExecutionSettings::enable_all()).await;
    let aggregate_report = report.aggregate.clone();
    let tally_report = report.tally.clone();

    // Number of retrieval reports should match number of sources
    assert_eq!(&report.retrieve.len(), &request.retrieve.len());

    for (index, retrieve_report) in report.retrieve.iter().enumerate() {
        // Each retrieval result must match last item in each retrieval partial results
        assert_eq!(
            &retrieve_report.result,
            retrieve_report
                .partial_results
                .clone()
                .unwrap()
                .last()
                .unwrap()
        );

        // Number of partial results for each source should match source's script length + 1
        assert_eq!(
            retrieve_report.partial_results.clone().unwrap().len(),
            unpack_radon_script(&request.retrieve.get(index).unwrap().script)
                .unwrap()
                .len()
                + 1
        );
    }

    // Number of aggregation partial results must equal number of filters + 2
    assert_eq!(
        report.aggregate.partial_results.unwrap().len(),
        &request.aggregate.filters.len() + 2
    );
    // Number of tally partial results must equal number of filters + 2
    assert_eq!(
        report.tally.partial_results.unwrap().len(),
        &request.tally.filters.len() + 2
    );

    // Aggregation result must match last item in aggregation partial results
    assert_eq!(
        &report.aggregate.result,
        aggregate_report.partial_results.unwrap().last().unwrap()
    );

    // Tally result must match last item in tally partial results
    assert_eq!(
        &report.tally.result,
        tally_report.partial_results.unwrap().last().unwrap()
    );
}
