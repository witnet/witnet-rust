use serde::Deserialize;
use std::collections::HashSet;
use std::convert::{TryFrom, TryInto};
use std::fs;
use witnet_data_structures::chain::DataRequestOutput;
use witnet_node::actors::messages::BuildDrt;
use witnet_rad::types::float::RadonFloat;
use witnet_rad::types::RadonTypes;

/// Id. Can be null, a number, or a string
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Id<'a> {
    Null,
    Number(u64),
    String(&'a str),
}
/// Generic request
#[derive(Debug, Deserialize)]
struct JsonRpcRequest<'a, T> {
    jsonrpc: &'a str,
    method: &'a str,
    id: Id<'a>,
    params: T,
}

#[test]
fn parse_examples() {
    let mut existing_examples: HashSet<&str> = vec!["bitcoin_price.json", "random_source.json"]
        .into_iter()
        .collect();
    for path in glob::glob("../examples/*.json").unwrap() {
        let path = path.unwrap();
        let v = path.file_name().unwrap().to_string_lossy();
        if !existing_examples.remove(v.as_ref()) {
            // The value did not exist before
            // Please create a test for it below and then manually add it to existing examples
            panic!("New example does not have test: {}", v);
        }
        println!("{}", path.display());
        let a = fs::read_to_string(path).unwrap();
        serde_json::from_str::<JsonRpcRequest<'_, BuildDrt>>(&a).unwrap();
    }

    if !existing_examples.is_empty() {
        panic!("Old examples no longer exist: {:?}", existing_examples);
    }
}

fn run_dr_locally_with_data(
    dr: &DataRequestOutput,
    data: &[&str],
) -> Result<RadonTypes, failure::Error> {
    let mut retrieval_results = vec![];
    for (r, d) in dr.data_request.retrieve.iter().zip(data.iter()) {
        log::info!("Running retrieval for {}", r.url);
        retrieval_results.push(witnet_rad::run_retrieval_with_data(r, d.to_string())?);
    }

    log::info!("Running aggregation with values {:?}", retrieval_results);
    let aggregation_result =
        witnet_rad::run_aggregation(retrieval_results, &dr.data_request.aggregate)?;
    log::info!("Aggregation result: {:?}", aggregation_result);

    // Assume that all the required witnesses will report the same value
    let reported_values = vec![aggregation_result; dr.witnesses.try_into().unwrap()]
        .into_iter()
        .map(|x| RadonTypes::try_from(x.as_slice()).unwrap())
        .collect();
    log::info!("Running tally with values {:?}", reported_values);
    let tally_result = witnet_rad::run_tally(reported_values, &dr.data_request.tally)?;
    log::info!("Tally result: {:?}", tally_result);

    Ok(RadonTypes::try_from(tally_result.as_slice())?)
}

fn test_dr(path: &str, data: &[&str], expected_result: RadonTypes) {
    let path = format!("../{}", path);
    let a = fs::read_to_string(&path).unwrap();
    let r = serde_json::from_str::<JsonRpcRequest<'_, BuildDrt>>(&a).unwrap();
    let x = run_dr_locally_with_data(&r.params.dro, data).unwrap();
    assert_eq!(x, expected_result);
}

#[test]
fn run_examples_bitcoin_price() {
    test_dr(
        "examples/bitcoin_price.json",
        &[r#"{"bpi":{"USD":{"rate_float":89279.0567}}}"#],
        RadonTypes::Float(RadonFloat::from(89279.0567)),
    );
}

#[test]
fn run_examples_random_source() {
    test_dr(
        "examples/random_source.json",
        &[r#"{"data":[5]}"#],
        RadonTypes::Float(RadonFloat::from(5.0)),
    );
}
