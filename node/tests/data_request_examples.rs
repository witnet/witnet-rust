use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    fs,
};

use serde::{Deserialize, Serialize};

use witnet_data_structures::{chain::DataRequestOutput, mainnet_validations::all_wips_active};
use witnet_node::actors::messages::BuildDrt;
use witnet_rad::{
    script::RadonScriptExecutionSettings,
    types::{
        bytes::RadonBytes, float::RadonFloat, integer::RadonInteger, string::RadonString,
        RadonTypes,
    },
};
use witnet_validations::validations::validate_rad_request;

/// Id. Can be null, a number, or a string
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Id<'a> {
    Null,
    Number(u64),
    String(&'a str),
}
/// Generic request
#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcRequest<'a, T> {
    jsonrpc: &'a str,
    method: &'a str,
    id: Id<'a>,
    params: T,
}

fn generate_example_json(build_drt: BuildDrt) -> String {
    serde_json::to_string(&JsonRpcRequest {
        jsonrpc: "2.0",
        method: "sendRequest",
        id: Id::String("1"),
        params: build_drt,
    })
    .unwrap()
}

fn run_dr_locally_with_data(
    dr: &DataRequestOutput,
    data: &[&str],
) -> Result<RadonTypes, failure::Error> {
    // Validate RADON: if the dr cannot be included in a witnet block, this should fail.
    // This does not validate other data request parameters such as number of witnesses, weight, or
    // collateral, so it is still possible that this request is considered invalid by miners.
    validate_rad_request(&dr.data_request, &all_wips_active())?;

    let mut retrieval_results = vec![];
    assert_eq!(dr.data_request.retrieve.len(), data.len());
    for (r, d) in dr.data_request.retrieve.iter().zip(data.iter()) {
        log::info!("Running retrieval for {}", r.url);
        retrieval_results.push(witnet_rad::run_retrieval_with_data(
            r,
            *d,
            RadonScriptExecutionSettings::disable_all(),
            all_wips_active(),
        )?);
    }

    log::info!("Running aggregation with values {:?}", retrieval_results);
    let aggregation_result = witnet_rad::run_aggregation(
        retrieval_results,
        &dr.data_request.aggregate,
        all_wips_active(),
    )?;
    log::info!("Aggregation result: {:?}", aggregation_result);

    // Assume that all the required witnesses will report the same value
    let reported_values: Result<Vec<RadonTypes>, _> =
        vec![aggregation_result; dr.witnesses.try_into().unwrap()]
            .into_iter()
            .map(RadonTypes::try_from)
            .collect();
    log::info!("Running tally with values {:?}", reported_values);
    let tally_result =
        witnet_rad::run_tally(reported_values?, &dr.data_request.tally, all_wips_active())?;
    log::info!("Tally result: {:?}", tally_result);

    Ok(tally_result)
}

#[test]
fn parse_examples() {
    let mut existing_examples = existing_examples();
    for path in glob::glob("../examples/*.json").unwrap() {
        let path = path.unwrap();
        let v = path.file_name().unwrap().to_string_lossy();
        match existing_examples.remove(v.as_ref()) {
            None => {
                // The value did not exist before
                // Please create a test for it below and then manually add it to existing_examples
                panic!("New example does not have test: {}", v);
            }
            Some((expected_dro, example_data, expected_result)) => {
                // This print is intentional, so when this test fails we know which example failed
                println!("{}", path.display());
                let a = fs::read_to_string(&path).unwrap();

                let file_value = match serde_json::from_str::<JsonRpcRequest<'_, BuildDrt>>(&a) {
                    Ok(x) if x.params == expected_dro => x,
                    _ => {
                        // If the contents do not match, or the JSON does not match the schema,
                        // print a nice message so the examples can be easily updated manually
                        panic!(
                            "Mismatch in test {}:\n\nFILE CONTENTS:\n{}\n\nEXPECTED:\n{}\n\n",
                            path.display(),
                            a,
                            generate_example_json(expected_dro)
                        )
                    }
                };

                // Run data request locally
                let local_result = run_dr_locally_with_data(&file_value.params.dro, example_data);
                assert_eq!(
                    local_result.unwrap(),
                    expected_result,
                    "Error when running data request example {}",
                    path.display()
                );
            }
        }
    }

    if !existing_examples.is_empty() {
        let eev: Vec<_> = existing_examples.keys().collect();
        panic!("Old examples no longer exist: {:?}", eev);
    }
}

/// List of existing example, with the expected (data request, input data, result).
/// When adding a new example, you need to manually retrieve the sources and paste the result of
/// the HTTP GET here, to avoid external queries in tests.
fn existing_examples() -> HashMap<&'static str, (BuildDrt, &'static [&'static str], RadonTypes)> {
    let a: Vec<(&str, BuildDrt, &[&str], RadonTypes)> = vec![
        (
            "bitcoin_price.json",
            examples::bitcoin_price(),
            &[
                r#"{"last":"89264.27"}"#,
                r#"{"bpi":{"USD":{"rate_float":89279.0567}}}"#,
                r#"{"data":{"markets":[{"ticker": {"lastPrice": "89261.012"}}]}}"#,
            ],
            RadonTypes::Float(RadonFloat::from(89268.11290000001)),
        ),
        (
            "error_301_source.json",
            examples::error_301_source(),
            &[r#"{"data":[5]}"#],
            RadonTypes::Float(RadonFloat::from(5.0)),
        ),
        (
            "random_source.json",
            examples::random_source(),
            &[r#"{"results":[{"dob":{"age":45}}]}"#],
            RadonTypes::Integer(RadonInteger::from(45)),
        ),
        (
            "bitcoin_last_hash.json",
            examples::bitcoin_last_hash(),
            &[
                r#"0000000000000000000e3b5418f6c92cb19494dfea28a83da8643485925aba1b"#,
                r#"{"hash":"0000000000000000000e3b5418f6c92cb19494dfea28a83da8643485925aba1b"}"#,
                r#"{"data":{"best_block_hash":"0000000000000000000e3b5418f6c92cb19494dfea28a83da8643485925aba1b"}}"#,
                r#"{"data":{"bitcoin":{"blocks": [{"blockHash": "0000000000000000000e3b5418f6c92cb19494dfea28a83da8643485925aba1b"}]}}}"#,
            ],
            RadonTypes::String(RadonString::from(
                "0000000000000000000e3b5418f6c92cb19494dfea28a83da8643485925aba1b",
            )),
        ),
        (
            "random_bytes.json",
            examples::random_bytes(),
            &["4"],
            RadonTypes::Bytes(RadonBytes::from(vec![
                37, 243, 87, 33, 196, 171, 163, 135, 8, 21, 38, 67, 130, 180, 217, 50, 108, 156,
                143, 166, 82, 161, 221, 100, 98, 226, 10, 230, 226, 213, 143, 190,
            ])),
        ),
        (
            "mojitoswap.json",
            examples::mojitoswap(),
            &[
                r#"{"data":{"bundles":[{"__typename":"Bundle","ethPrice":"23.2510022494185052280717086560108","id":"1"}]}}"#,
            ],
            RadonTypes::Float(RadonFloat::from(23.251002249418505)),
        ),
        (
            "xml_source.json",
            examples::xml_source(),
            &[
                r#"{"image_data":"<svg xmlns='http://www.w3.org/2000/svg'><path fill='#ea2'/><path fill='#fe2'/></svg>"}"#,
            ],
            RadonTypes::String(RadonString::from("#ea2")),
        ),
        (
            "xml_source2.json",
            examples::xml_source2(),
            &[r#"<?xml version="1.0" encoding="ISO-8859-1"?>
                    <dwml version="1.0" xmlns:xsd="https://www.w3.org/2001/XMLSchema" xmlns:xsi="https://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="https://graphical.weather.gov/xml/DWMLgen/schema/DWML.xsd">
                        <data type="forecast">
                            <parameters applicable-location="point1">
                                <weather time-layout="k-p12h-n13-1">
                                    <name>Weather Type, Coverage, Intensity</name>
                                    <weather-conditions weather-summary="Partly Sunny then Chance Rain/Snow"/>
                                    <weather-conditions weather-summary="Snow Likely and Blustery"/>
                                    <weather-conditions weather-summary="Snow Likely"/>
                                </weather>
                            </parameters>
                        </data>
                        <data type="current observations">
	                        <location>
		                        <location-key>point1</location-key>
		                        <point latitude="39.9" longitude="-105.1"/>
	                        </location>
	                    </data>
	                </dwml>">"#],
            RadonTypes::String(RadonString::from("Snow Likely")),
        ),
    ];

    a.into_iter().map(|t| (t.0, (t.1, t.2, t.3))).collect()
}

mod examples {
    use witnet_data_structures::chain::{
        DataRequestOutput, RADAggregate, RADFilter, RADRequest, RADRetrieve, RADTally, RADType,
    };
    use witnet_node::actors::messages::BuildDrt;
    use witnet_rad::{
        cbor_to_vec, filters::RadonFilters, operators::RadonOpCodes, reducers::RadonReducers,
        CborValue as Value,
    };

    pub fn bitcoin_price() -> BuildDrt {
        let url_0 = "https://www.bitstamp.net/api/ticker/";
        let r0_script = cbor_to_vec(&Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONMap as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetFloat as i128),
                Value::Text(String::from("last")),
            ]),
        ]))
        .unwrap();

        let url_1 = "https://api.coindesk.com/v1/bpi/currentprice.json";
        let r1_script = cbor_to_vec(&Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONMap as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text(String::from("bpi")),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text(String::from("USD")),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetFloat as i128),
                Value::Text(String::from("rate_float")),
            ]),
        ]))
        .unwrap();

        let url_2 = "https://api.blocktap.io/graphql";
        let r2_script = cbor_to_vec(&Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONMap as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text("data".to_string()),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetArray as i128),
                Value::Text("markets".to_string()),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::ArrayGetMap as i128),
                Value::Integer(0),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text("ticker".to_string()),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetString as i128),
                Value::Text("lastPrice".to_string()),
            ]),
            Value::Integer(RadonOpCodes::StringAsFloat as i128),
        ]))
        .unwrap();
        let r2_body = Vec::from(String::from(
            r#"{"query":"query price {\n  markets(filter:{ baseSymbol: {_eq:\"BTC\"} quoteSymbol: {_in:[\"USD\",\"USDT\"]} marketStatus: { _eq: Active }}) {\n    marketSymbol\n    ticker {\n      lastPrice\n    }\n  }\n}","variables":null,"operationName":"price"}"#,
        ));
        let r2_headers = vec![("Content-Type", "application/json")];
        let r2_headers = r2_headers
            .into_iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();

        BuildDrt {
            dro: DataRequestOutput {
                data_request: RADRequest {
                    time_lock: 1_574_703_683,
                    retrieve: vec![
                        RADRetrieve {
                            kind: RADType::HttpGet,
                            url: url_0.to_string(),
                            script: r0_script,
                            body: vec![],
                            headers: vec![],
                        },
                        RADRetrieve {
                            kind: RADType::HttpGet,
                            url: url_1.to_string(),
                            script: r1_script,
                            body: vec![],
                            headers: vec![],
                        },
                        RADRetrieve {
                            kind: RADType::HttpPost,
                            url: url_2.to_string(),
                            script: r2_script,
                            body: r2_body,
                            headers: r2_headers,
                        },
                    ],
                    aggregate: RADAggregate {
                        filters: vec![],
                        reducer: RadonReducers::AverageMean as u32,
                    },
                    tally: RADTally {
                        filters: vec![],
                        reducer: RadonReducers::AverageMean as u32,
                    },
                },
                witness_reward: 1000,
                witnesses: 2,
                commit_and_reveal_fee: 1,
                min_consensus_percentage: 51,
                collateral: 1_000_000_000,
            },
            fee: 10,
        }
    }

    pub fn random_source() -> BuildDrt {
        let url_0 = "https://randomuser.me/api/";
        let r0_script = cbor_to_vec(&Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONMap as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetArray as i128),
                Value::Text(String::from("results")),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::ArrayGetMap as i128),
                Value::Integer(0),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text(String::from("dob")),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetFloat as i128),
                Value::Text(String::from("age")),
            ]),
            Value::Integer(RadonOpCodes::FloatRound as i128),
        ]))
        .unwrap();

        BuildDrt {
            dro: DataRequestOutput {
                data_request: RADRequest {
                    time_lock: 0,
                    retrieve: vec![RADRetrieve {
                        kind: RADType::HttpGet,
                        url: url_0.to_string(),
                        script: r0_script,
                        body: vec![],
                        headers: vec![],
                    }],
                    aggregate: RADAggregate {
                        filters: vec![],
                        reducer: RadonReducers::AverageMean as u32,
                    },
                    tally: RADTally {
                        filters: vec![RADFilter {
                            op: RadonFilters::DeviationStandard as u32,
                            args: vec![249, 60, 0],
                        }],
                        reducer: RadonReducers::AverageMean as u32,
                    },
                },
                witness_reward: 1000,
                witnesses: 4,
                commit_and_reveal_fee: 10,
                min_consensus_percentage: 51,
                collateral: 1_000_000_000,
            },
            fee: 0,
        }
    }

    pub fn error_301_source() -> BuildDrt {
        let url_0 =
            "http://www.skyverge.com/woocommerce–rest–api-docs.html#authentication/over-https";
        let r0_script = cbor_to_vec(&Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONMap as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetArray as i128),
                Value::Text(String::from("data")),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::ArrayGetFloat as i128),
                Value::Integer(0),
            ]),
        ]))
        .unwrap();

        BuildDrt {
            dro: DataRequestOutput {
                data_request: RADRequest {
                    time_lock: 0,
                    retrieve: vec![RADRetrieve {
                        kind: RADType::HttpGet,
                        url: url_0.to_string(),
                        script: r0_script,
                        body: vec![],
                        headers: vec![],
                    }],
                    aggregate: RADAggregate {
                        filters: vec![],
                        reducer: RadonReducers::AverageMean as u32,
                    },
                    tally: RADTally {
                        filters: vec![],
                        reducer: RadonReducers::AverageMean as u32,
                    },
                },
                witness_reward: 1000,
                witnesses: 2,
                commit_and_reveal_fee: 5,
                min_consensus_percentage: 51,
                collateral: 1_000_000_000,
            },
            fee: 0,
        }
    }

    pub fn bitcoin_last_hash() -> BuildDrt {
        let url_0 = "https://blockchain.info/q/latesthash";
        let r0_script = cbor_to_vec(&Value::Array(vec![])).unwrap();

        let url_1 = "https://api-r.bitcoinchain.com/v1/status";
        let r1_script = cbor_to_vec(&Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONMap as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetString as i128),
                Value::Text(String::from("hash")),
            ]),
        ]))
        .unwrap();

        let url_2 = "https://api.blockchair.com/bitcoin/stats";
        let r2_script = cbor_to_vec(&Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONMap as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text(String::from("data")),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetString as i128),
                Value::Text(String::from("best_block_hash")),
            ]),
        ]))
        .unwrap();

        let url_3 = "https://graphql.bitquery.io/";
        let r3_script = cbor_to_vec(&Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONMap as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text("data".to_string()),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text("bitcoin".to_string()),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetArray as i128),
                Value::Text("blocks".to_string()),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::ArrayGetMap as i128),
                Value::Integer(0),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetString as i128),
                Value::Text("blockHash".to_string()),
            ]),
        ]))
        .unwrap();
        let r3_body = Vec::from(String::from(
            r#"{"query":"query{\nbitcoin {\nblocks(options: {limit: 1, desc: \"height\"}) {\nheight\nblockHash\n}\n}\n}","variables":null}"#,
        ));
        // Many headers are needed
        let r3_headers = vec![
            ("Accept", "*/*"),
            ("Referer", "https://bitquery.io/"),
            ("Content-Type", "application/json"),
            ("Origin", "https://bitquery.io"),
        ];
        let r3_headers = r3_headers
            .into_iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();

        BuildDrt {
            dro: DataRequestOutput {
                data_request: RADRequest {
                    time_lock: 0,
                    retrieve: vec![
                        RADRetrieve {
                            kind: RADType::HttpGet,
                            url: url_0.to_string(),
                            script: r0_script,
                            body: vec![],
                            headers: vec![],
                        },
                        RADRetrieve {
                            kind: RADType::HttpGet,
                            url: url_1.to_string(),
                            script: r1_script,
                            body: vec![],
                            headers: vec![],
                        },
                        RADRetrieve {
                            kind: RADType::HttpGet,
                            url: url_2.to_string(),
                            script: r2_script,
                            body: vec![],
                            headers: vec![],
                        },
                        RADRetrieve {
                            kind: RADType::HttpPost,
                            url: url_3.to_string(),
                            script: r3_script,
                            body: r3_body,
                            headers: r3_headers,
                        },
                    ],
                    aggregate: RADAggregate {
                        filters: vec![],
                        reducer: RadonReducers::Mode as u32,
                    },
                    tally: RADTally {
                        filters: vec![RADFilter {
                            op: RadonFilters::Mode as u32,
                            args: vec![],
                        }],
                        reducer: RadonReducers::Mode as u32,
                    },
                },
                witness_reward: 1000,
                witnesses: 3,
                commit_and_reveal_fee: 10,
                min_consensus_percentage: 51,
                collateral: 1_000_000_000,
            },
            fee: 0,
        }
    }

    pub fn random_bytes() -> BuildDrt {
        let url_0 = "";
        let r0_script = vec![128];

        BuildDrt {
            dro: DataRequestOutput {
                data_request: RADRequest {
                    time_lock: 0,
                    retrieve: vec![RADRetrieve {
                        kind: RADType::Rng,
                        url: url_0.to_string(),
                        script: r0_script,
                        body: vec![],
                        headers: vec![],
                    }],
                    aggregate: RADAggregate {
                        filters: vec![],
                        reducer: RadonReducers::Mode as u32,
                    },
                    tally: RADTally {
                        filters: vec![],
                        reducer: RadonReducers::HashConcatenate as u32,
                    },
                },
                witness_reward: 1000,
                witnesses: 5,
                commit_and_reveal_fee: 10,
                min_consensus_percentage: 51,
                collateral: 1_000_000_000,
            },
            fee: 0,
        }
    }

    pub fn mojitoswap() -> BuildDrt {
        let url_0 = "https://thegraph.kcc.network/subgraphs/name/mojito/swap";
        let r0_script = cbor_to_vec(&Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONMap as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text("data".to_string()),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetArray as i128),
                Value::Text("bundles".to_string()),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::ArrayGetMap as i128),
                Value::Integer(0),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetString as i128),
                Value::Text("ethPrice".to_string()),
            ]),
            Value::Integer(RadonOpCodes::StringAsFloat as i128),
        ]))
        .unwrap();
        let r0_body = Vec::from(String::from(
            r#"{"operationName":"bundles","variables":{},"query":"query bundles {\n  bundles(where: {id: 1}) {\n    id\n    ethPrice\n    __typename\n  }\n}\n"}"#,
        ));
        let r0_headers = vec![("Content-Type", "application/json")];
        let r0_headers = r0_headers
            .into_iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();

        BuildDrt {
            dro: DataRequestOutput {
                data_request: RADRequest {
                    time_lock: 0,
                    retrieve: vec![RADRetrieve {
                        kind: RADType::HttpPost,
                        url: url_0.to_string(),
                        script: r0_script,
                        body: r0_body,
                        headers: r0_headers,
                    }],
                    aggregate: RADAggregate {
                        filters: vec![],
                        reducer: RadonReducers::AverageMean as u32,
                    },
                    tally: RADTally {
                        filters: vec![],
                        reducer: RadonReducers::AverageMean as u32,
                    },
                },
                witness_reward: 1000,
                witnesses: 3,
                commit_and_reveal_fee: 10,
                min_consensus_percentage: 51,
                collateral: 1_000_000_000,
            },
            fee: 0,
        }
    }

    pub fn xml_source() -> BuildDrt {
        let url_0 = "https://api-liscon21.wittycreatures.com/metadata/1";
        let r0_script = cbor_to_vec(&Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONMap as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetString as i128),
                Value::Text(String::from("image_data")),
            ]),
            Value::Integer(RadonOpCodes::StringParseXMLMap as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text(String::from("svg")),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetArray as i128),
                Value::Text(String::from("path")),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::ArrayGetMap as i128),
                Value::Integer(0),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetString as i128),
                Value::Text(String::from("@fill")),
            ]),
        ]))
        .unwrap();

        BuildDrt {
            dro: DataRequestOutput {
                data_request: RADRequest {
                    time_lock: 0,
                    retrieve: vec![RADRetrieve {
                        kind: RADType::HttpGet,
                        url: url_0.to_string(),
                        script: r0_script,
                        body: vec![],
                        headers: vec![],
                    }],
                    aggregate: RADAggregate {
                        filters: vec![],
                        reducer: RadonReducers::Mode as u32,
                    },
                    tally: RADTally {
                        filters: vec![RADFilter {
                            op: RadonFilters::Mode as u32,
                            args: vec![],
                        }],
                        reducer: RadonReducers::Mode as u32,
                    },
                },
                witness_reward: 1000,
                witnesses: 3,
                commit_and_reveal_fee: 10,
                min_consensus_percentage: 51,
                collateral: 1_000_000_000,
            },
            fee: 0,
        }
    }

    pub fn xml_source2() -> BuildDrt {
        let url_0 = "https://forecast.weather.gov/MapClick.php?lat=39.75&lon=-104.99&unit=0&lg=english&FcstType=dwml";
        let r0_script = cbor_to_vec(&Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseXMLMap as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text(String::from("dwml")),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetArray as i128),
                Value::Text(String::from("data")),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::ArrayGetMap as i128),
                Value::Integer(0),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text(String::from("parameters")),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text(String::from("weather")),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetArray as i128),
                Value::Text(String::from("weather-conditions")),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::ArrayGetMap as i128),
                Value::Integer(2),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetString as i128),
                Value::Text(String::from("@weather-summary")),
            ]),
        ]))
        .unwrap();

        BuildDrt {
            dro: DataRequestOutput {
                data_request: RADRequest {
                    time_lock: 0,
                    retrieve: vec![RADRetrieve {
                        kind: RADType::HttpGet,
                        url: url_0.to_string(),
                        script: r0_script,
                        body: vec![],
                        headers: vec![],
                    }],
                    aggregate: RADAggregate {
                        filters: vec![],
                        reducer: RadonReducers::Mode as u32,
                    },
                    tally: RADTally {
                        filters: vec![RADFilter {
                            op: RadonFilters::Mode as u32,
                            args: vec![],
                        }],
                        reducer: RadonReducers::Mode as u32,
                    },
                },
                witness_reward: 1000,
                witnesses: 3,
                commit_and_reveal_fee: 10,
                min_consensus_percentage: 51,
                collateral: 1_000_000_000,
            },
            fee: 1000,
        }
    }
}
