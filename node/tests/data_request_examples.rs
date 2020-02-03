use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    fs,
};
use witnet_data_structures::chain::DataRequestOutput;
use witnet_node::actors::messages::BuildDrt;
use witnet_rad::types::{float::RadonFloat, string::RadonString, RadonTypes};

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
    let mut retrieval_results = vec![];
    for (r, d) in dr.data_request.retrieve.iter().zip(data.iter()) {
        log::info!("Running retrieval for {}", r.url);
        retrieval_results.push(witnet_rad::run_retrieval_with_data(r, (*d).to_string())?);
    }

    log::info!("Running aggregation with values {:?}", retrieval_results);
    let aggregation_result =
        witnet_rad::run_aggregation(retrieval_results, &dr.data_request.aggregate)?;
    log::info!("Aggregation result: {:?}", aggregation_result);

    // Assume that all the required witnesses will report the same value
    let reported_values: Result<Vec<RadonTypes>, _> =
        vec![aggregation_result; dr.witnesses.try_into().unwrap()]
            .into_iter()
            .map(RadonTypes::try_from)
            .collect();
    log::info!("Running tally with values {:?}", reported_values);
    let tally_result = witnet_rad::run_tally(reported_values?, &dr.data_request.tally)?;
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
            ],
            RadonTypes::Float(RadonFloat::from(89271.66335)),
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
            &[r#"[{"status":"success","min":0,"max":100,"random":31}]"#],
            RadonTypes::Float(RadonFloat::from(31.0)),
        ),
        (
            "bitcoin_last_hash.json",
            examples::bitcoin_last_hash(),
            &[
                r#"0000000000000000000e3b5418f6c92cb19494dfea28a83da8643485925aba1b"#,
                r#"{"hash":"0000000000000000000e3b5418f6c92cb19494dfea28a83da8643485925aba1b"}"#,
                r#"{"data":{"best_block_hash":"0000000000000000000e3b5418f6c92cb19494dfea28a83da8643485925aba1b"}}"#,
            ],
            RadonTypes::String(RadonString::from(
                "0000000000000000000e3b5418f6c92cb19494dfea28a83da8643485925aba1b",
            )),
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

        BuildDrt {
            dro: DataRequestOutput {
                data_request: RADRequest {
                    time_lock: 1_574_703_683,
                    retrieve: vec![
                        RADRetrieve {
                            kind: RADType::HttpGet,
                            url: url_0.to_string(),
                            script: r0_script,
                        },
                        RADRetrieve {
                            kind: RADType::HttpGet,
                            url: url_1.to_string(),
                            script: r1_script,
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
                backup_witnesses: 2,
                commit_fee: 1,
                reveal_fee: 1,
                tally_fee: 1,
                extra_commit_rounds: 3,
                extra_reveal_rounds: 3,
                min_consensus_percentage: 51,
            },
            fee: 10,
        }
    }

    pub fn random_source() -> BuildDrt {
        let url_0 = "https://csrng.net/csrng/csrng.php?min=0&max=100";
        let r0_script = cbor_to_vec(&Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONArray as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::ArrayGetMap as i128),
                Value::Integer(0),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetFloat as i128),
                Value::Text(String::from("random")),
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
                backup_witnesses: 1,
                commit_fee: 10,
                reveal_fee: 10,
                tally_fee: 40,
                extra_commit_rounds: 1,
                extra_reveal_rounds: 1,
                min_consensus_percentage: 51,
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
                backup_witnesses: 1,
                commit_fee: 5,
                reveal_fee: 5,
                tally_fee: 10,
                extra_commit_rounds: 2,
                extra_reveal_rounds: 2,
                min_consensus_percentage: 51,
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

        BuildDrt {
            dro: DataRequestOutput {
                data_request: RADRequest {
                    time_lock: 0,
                    retrieve: vec![
                        RADRetrieve {
                            kind: RADType::HttpGet,
                            url: url_0.to_string(),
                            script: r0_script,
                        },
                        RADRetrieve {
                            kind: RADType::HttpGet,
                            url: url_1.to_string(),
                            script: r1_script,
                        },
                        RADRetrieve {
                            kind: RADType::HttpGet,
                            url: url_2.to_string(),
                            script: r2_script,
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
                backup_witnesses: 1,
                commit_fee: 10,
                reveal_fee: 10,
                tally_fee: 30,
                extra_commit_rounds: 1,
                extra_reveal_rounds: 1,
                min_consensus_percentage: 51,
            },
            fee: 0,
        }
    }
}
