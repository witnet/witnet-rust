use std::net::SocketAddr;
use std::str::FromStr;
use std::{
    fmt,
    io::{self, BufRead, BufReader, Read, Write},
    net::TcpStream,
};

use failure::{bail, Fail};
use serde::Deserialize;
use serde_json::json;

use itertools::Itertools;
use log::*;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use witnet_data_structures::chain::{
    DataRequestOutput, Environment, OutputPointer, PublicKeyHash, Reputation, ValueTransferOutput,
};
use witnet_data_structures::proto::ProtobufConvert;
use witnet_node::actors::{json_rpc::json_rpc_methods::GetBlockChainParams, messages::BuildVtt};
use witnet_rad::types::RadonTypes;
use witnet_validations::validations::{validate_data_request_output, validate_rad_request};

pub fn raw(addr: SocketAddr) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    // The request is read from stdin, one line at a time
    let mut request = String::new();
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    loop {
        request.clear();
        let count = stdin.read_line(&mut request)?;
        if count == 0 {
            break Ok(());
        }
        let response = send_request(&mut stream, &request)?;
        // The response includes a newline, so use print instead of println
        print!("{}", response);
    }
}

pub fn get_blockchain(addr: SocketAddr, epoch: i64, limit: u32) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let params = GetBlockChainParams { epoch, limit };
    let response = send_request(
        &mut stream,
        &format!(
            r#"{{"jsonrpc": "2.0","method": "getBlockChain", "params": {}, "id": 1}}"#,
            serde_json::to_string(&params).unwrap()
        ),
    )?;
    log::info!("{}", response);
    let block_chain: ResponseBlockChain<'_> = parse_response(&response)?;

    for (epoch, hash) in block_chain {
        println!("block for epoch #{} had digest {}", epoch, hash);
    }

    Ok(())
}

pub fn get_balance(addr: SocketAddr, pkh: Option<PublicKeyHash>) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;

    let pkh = match pkh {
        Some(pkh) => pkh,
        None => {
            log::info!("No pkh specified, will default to node pkh");
            let request = r#"{"jsonrpc": "2.0","method": "getPkh", "id": "1"}"#;
            let response = send_request(&mut stream, &request)?;
            let node_pkh = parse_response::<PublicKeyHash>(&response)?;
            log::info!("Node pkh: {}", node_pkh);

            node_pkh
        }
    };

    let request = format!(
        r#"{{"jsonrpc": "2.0","method": "getBalance", "params": [{}], "id": "1"}}"#,
        serde_json::to_string(&pkh)?,
    );
    let response = send_request(&mut stream, &request)?;
    log::info!("{}", response);
    let amount = parse_response::<u64>(&response)?;

    println!("{}", amount);

    Ok(())
}

pub fn get_pkh(addr: SocketAddr) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let request = r#"{"jsonrpc": "2.0","method": "getPkh", "id": "1"}"#;
    let response = send_request(&mut stream, &request)?;
    log::info!("{}", response);
    let pkh = parse_response::<PublicKeyHash>(&response)?;

    println!("{}", pkh);
    println!("Testnet address: {}", pkh.bech32(Environment::Testnet1));
    println!("Mainnet address: {}", pkh.bech32(Environment::Mainnet));

    Ok(())
}

pub fn get_reputation(
    addr: SocketAddr,
    pkh: Option<PublicKeyHash>,
    all: bool,
) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;

    if all {
        let request = r#"{"jsonrpc": "2.0","method": "getReputationAll", "id": "1"}"#;
        let response = send_request(&mut stream, &request)?;
        let rep_map = parse_response::<HashMap<PublicKeyHash, (Reputation, bool)>>(&response)?;
        println!("Total Reputation: {{");
        for (pkh, (rep, active)) in rep_map
            .into_iter()
            .sorted_by(|a, b| a.0.to_string().cmp(&b.0.to_string()))
        {
            let active = if active { 'A' } else { ' ' };
            println!("    [{}] {}: {}", active, pkh, rep.0);
        }
        println!("}}");
        return Ok(());
    }

    let pkh = match pkh {
        Some(pkh) => pkh,
        None => {
            log::info!("No pkh specified, will default to node pkh");
            let request = r#"{"jsonrpc": "2.0","method": "getPkh", "id": "1"}"#;
            let response = send_request(&mut stream, &request)?;
            let node_pkh = parse_response::<PublicKeyHash>(&response)?;
            log::info!("Node pkh: {}", node_pkh);

            node_pkh
        }
    };

    let request = format!(
        r#"{{"jsonrpc": "2.0","method": "getReputation", "params": [{}], "id": "1"}}"#,
        serde_json::to_string(&pkh)?,
    );
    let response = send_request(&mut stream, &request)?;
    log::info!("{}", response);
    let (amount, active) = parse_response::<(Reputation, bool)>(&response)?;

    println!(
        "Identity {} has {} reputation and is {}",
        pkh,
        amount.0,
        if active { "active" } else { "not active" }
    );

    Ok(())
}

pub fn get_block(addr: SocketAddr, hash: String) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let request = format!(
        r#"{{"jsonrpc": "2.0","method": "getBlock", "params": [{:?}], "id": "1"}}"#,
        hash,
    );
    let response = send_request(&mut stream, &request)?;

    println!("{}", response);

    Ok(())
}

pub fn get_output(addr: SocketAddr, pointer: String) -> Result<(), failure::Error> {
    let mut _stream = start_client(addr)?;
    let output_pointer = OutputPointer::from_str(&pointer)?;
    let request_payload = serde_json::to_string(&output_pointer)?;
    let _request = format!(
        r#"{{"jsonrpc": "2.0","method": "getOutput", "params": [{}], "id": "1"}}"#,
        request_payload,
    );
    //let response = send_request(&mut stream, &request)?;
    let response = "unimplemented yet";

    println!("{}", response);

    Ok(())
}

pub fn send_vtt(
    addr: SocketAddr,
    pkh: PublicKeyHash,
    value: u64,
    fee: u64,
    time_lock: u64,
) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let params = BuildVtt {
        vto: vec![ValueTransferOutput {
            pkh,
            value,
            time_lock,
        }],
        fee,
    };
    let request = format!(
        r#"{{"jsonrpc": "2.0","method": "sendValue", "params": {}, "id": "1"}}"#,
        serde_json::to_string(&params)?
    );
    let response = send_request(&mut stream, &request)?;

    println!("{}", response);

    Ok(())
}

fn run_dr_locally(dr: &DataRequestOutput) -> Result<RadonTypes, failure::Error> {
    // Block on data request retrieval because the CLI application blocks everywhere anyway
    let run_retrieval_blocking =
        |retrieve| futures03::executor::block_on(witnet_rad::run_retrieval(retrieve));

    let mut retrieval_results = vec![];
    for r in &dr.data_request.retrieve {
        log::info!("Running retrieval for {}", r.url);
        retrieval_results.push(run_retrieval_blocking(r)?);
    }

    log::info!("Running aggregation with values {:?}", retrieval_results);
    let aggregation_result =
        witnet_rad::run_aggregation(retrieval_results, &dr.data_request.aggregate)?;
    log::info!("Aggregation result: {:?}", aggregation_result);

    // Assume that all the required witnesses will report the same value
    let reported_values: Result<Vec<RadonTypes>, _> =
        vec![aggregation_result; dr.witnesses.try_into()?]
            .into_iter()
            .map(RadonTypes::try_from)
            .collect();
    log::info!("Running tally with values {:?}", reported_values);
    let tally_result = witnet_rad::run_tally(reported_values?, &dr.data_request.tally)?;
    log::info!("Tally result: {:?}", tally_result);

    Ok(RadonTypes::try_from(tally_result)?)
}

fn deserialize_and_validate_hex_dr(hex_bytes: String) -> Result<DataRequestOutput, failure::Error> {
    let dr_bytes = hex::decode(hex_bytes)?;

    let dr: DataRequestOutput = ProtobufConvert::from_pb_bytes(&dr_bytes)?;

    debug!("{}", serde_json::to_string(&dr)?);

    validate_data_request_output(&dr)?;
    validate_rad_request(&dr.data_request)?;

    // Is the data request serialized correctly?
    // Check that serializing the deserialized struct results in exactly the same bytes
    let witnet_dr_bytes = dr.to_pb_bytes()?;

    if dr_bytes != witnet_dr_bytes {
        warn!("Data request uses an invalid serialization, will be ignored.\nINPUT BYTES: {:02x?}\nWIT DR BYTES: {:02x?}",
              dr_bytes, witnet_dr_bytes
        );
        warn!(
            "This usually happens when some fields are set to 0. \
             The Rust implementation of ProtocolBuffer skips those fields, \
             as missing fields are deserialized with the default value."
        );
        bail!("Invalid serialization");
    }

    Ok(dr)
}

pub fn send_dr(
    addr: SocketAddr,
    hex_bytes: String,
    fee: u64,
    run: bool,
) -> Result<(), failure::Error> {
    let dr_output = deserialize_and_validate_hex_dr(hex_bytes)?;
    if run {
        run_dr_locally(&dr_output)?;
    }

    let bdr_params = json!({"dro": dr_output, "fee": fee});
    let request = format!(
        r#"{{"jsonrpc": "2.0","method": "sendRequest", "params": {}, "id": "1"}}"#,
        serde_json::to_string(&bdr_params)?
    );
    let mut stream = start_client(addr)?;
    let response = send_request(&mut stream, &request)?;

    println!("{}", response);

    Ok(())
}

// Response of the getBlockChain JSON-RPC method
type ResponseBlockChain<'a> = Vec<(u32, &'a str)>;

// Quick and simple JSON-RPC client implementation

/// Generic response which is used to extract the result
#[derive(Debug, Deserialize)]
struct JsonRpcResponse<'a, T> {
    // Lifetimes allow zero-copy string deserialization
    jsonrpc: &'a str,
    id: Id<'a>,
    result: T,
}

/// A failed request returns an error with code and message
#[derive(Debug, Deserialize)]
struct JsonRpcError<'a> {
    jsonrpc: &'a str,
    id: Id<'a>,
    error: ServerError,
}

/// Id. Can be null, a number, or a string
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Id<'a> {
    Null,
    Number(u64),
    String(&'a str),
}

/// A failed request returns an error with code and message
#[derive(Debug, Deserialize, Fail)]
struct ServerError {
    code: i32,
    // This cannot be a &str because the error may outlive the current function
    message: String,
}

#[derive(Debug, Fail)]
struct ProtocolError(String);

// Required for Fail derive
impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&format!("{:?}", self))?;
        Ok(())
    }
}

// Required for Fail derive
impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&format!(
            "Incompatible JSON-RPC version used by server: {}",
            self.0
        ))?;
        Ok(())
    }
}

fn start_client(addr: SocketAddr) -> Result<TcpStream, failure::Error> {
    log::info!("Connecting to JSON-RPC server at {}", addr);
    let stream = TcpStream::connect(addr);

    stream.map_err(Into::into)
}

fn send_request<S: Read + Write>(stream: &mut S, request: &str) -> Result<String, io::Error> {
    stream.write_all(request.as_bytes())?;
    // Write missing newline, if needed
    match bytecount::count(request.as_bytes(), b'\n') {
        0 => stream.write_all(b"\n")?,
        1 => {}
        _ => {
            log::warn!("The request contains more than one newline, only the first response will be returned");
        }
    }
    // Read only one line
    let mut r = BufReader::new(stream);
    let mut buf = String::new();
    r.read_line(&mut buf)?;
    Ok(buf)
}

fn parse_response<'a, T: Deserialize<'a>>(response: &'a str) -> Result<T, failure::Error> {
    match serde_json::from_str::<JsonRpcResponse<'a, T>>(response) {
        Ok(x) => {
            // x.id should also be checked if we want to support more than one call at a time
            if x.jsonrpc != "2.0" {
                Err(ProtocolError(x.jsonrpc.to_string()).into())
            } else {
                Ok(x.result)
            }
        }
        Err(e) => {
            log::info!("{}", e);
            let error_json: JsonRpcError<'a> = serde_json::from_str(response)?;
            Err(error_json.error.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_invalid() {
        let nothing: Result<(), _> = parse_response("");
        assert!(nothing.is_err());
        let asdf: Result<(), _> = parse_response("asdf");
        assert!(asdf.is_err());
    }

    #[test]
    fn parse_server_error() {
        let response =
            r#"{"jsonrpc":"2.0","error":{"code":-32601,"message":"Method not found"},"id":1}"#;
        let block_chain: Result<ResponseBlockChain<'_>, _> = parse_response(&response);
        assert!(block_chain.is_err());
    }

    #[test]
    fn parse_get_block_chain() {
        let response = r#"{"jsonrpc":"2.0","result":[[0,"ed28899af8c3148a4162736af942bc68c4466da93c5124dabfaa7c582af49e30"],[1,"9c9038cfb31a7050796920f91b17f4a68c7e9a795ee8962916b35d39fc1efefc"]],"id":1}"#;
        let block_chain: ResponseBlockChain<'_> = parse_response(&response).unwrap();
        assert_eq!(
            block_chain[0],
            (
                0,
                "ed28899af8c3148a4162736af942bc68c4466da93c5124dabfaa7c582af49e30"
            )
        );
        assert_eq!(
            block_chain[1],
            (
                1,
                "9c9038cfb31a7050796920f91b17f4a68c7e9a795ee8962916b35d39fc1efefc"
            )
        );
    }
}
