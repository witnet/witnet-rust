use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    fmt,
    io::{self, BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    str::FromStr,
};

use ansi_term::Color::{Purple, Red, White, Yellow};
use failure::{bail, Fail};
use itertools::Itertools;
use prettytable::{cell, row, Table};
use serde::{Deserialize, Serialize};
use serde_json::json;
use witnet_crypto::key::{CryptoEngine, ExtendedPK, ExtendedSK};
use witnet_data_structures::{
    chain::{
        DataRequestInfo, DataRequestOutput, Environment, OutputPointer, PublicKey, PublicKeyHash,
        Reputation, UtxoInfo, UtxoSelectionStrategy, ValueTransferOutput,
    },
    proto::ProtobufConvert,
    transaction::Transaction,
};
use witnet_node::actors::{
    json_rpc::json_rpc_methods::{
        AddrType, GetBlockChainParams, GetTransactionOutput, PeersResult,
    },
    messages::BuildVtt,
};
use witnet_rad::types::RadonTypes;
use witnet_util::{credentials::create_credentials_file, timestamp::pretty_print};
use witnet_validations::validations::{validate_data_request_output, validate_rad_request, Wit};

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

pub fn get_blockchain(addr: SocketAddr, epoch: i64, limit: i64) -> Result<(), failure::Error> {
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

    println!("{} wits", Wit::from_nanowits(amount));

    Ok(())
}

pub fn get_pkh(addr: SocketAddr) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let request = r#"{"jsonrpc": "2.0","method": "getPkh", "id": "1"}"#;
    let response = send_request(&mut stream, &request)?;
    log::info!("{}", response);
    let pkh = parse_response::<PublicKeyHash>(&response)?;

    println!("{}", pkh);
    println!("Testnet address: {}", pkh.bech32(Environment::Testnet));
    println!("Mainnet address: {}", pkh.bech32(Environment::Mainnet));

    Ok(())
}

#[allow(clippy::cast_possible_wrap)]
pub fn get_utxo_info(
    addr: SocketAddr,
    long: bool,
    pkh: Option<PublicKeyHash>,
) -> Result<(), failure::Error> {
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
        r#"{{"jsonrpc": "2.0","method": "getUtxoInfo", "params": [{}], "id": "1"}}"#,
        serde_json::to_string(&pkh)?,
    );
    let response = send_request(&mut stream, &request)?;
    let utxo_info = parse_response::<UtxoInfo>(&response)?;

    let utxos_len = utxo_info.utxos.len();

    if long {
        let mut table = Table::new();
        table.set_format(*prettytable::format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
        table.set_titles(row![
            "OutputPointer",
            "Value (in wits)",
            "Time lock",
            "Ready for collateral"
        ]);
        for utxo_metadata in utxo_info
            .utxos
            .into_iter()
            .sorted_by_key(|um| (um.value, um.output_pointer.clone()))
        {
            let value = Wit::from_nanowits(utxo_metadata.value).to_string();
            let time_lock = if utxo_metadata.timelock == 0 {
                "Ready".to_string()
            } else {
                let date = pretty_print(utxo_metadata.timelock as i64, 0);
                format!("{:?}", date)
            };
            table.add_row(row![
                utxo_metadata.output_pointer.to_string(),
                value,
                time_lock,
                utxo_metadata.ready_for_collateral.to_string()
            ]);
        }
        table.printstd();
        println!("-----------------------");
    }
    println!("Total number of utxos: {}", utxos_len);
    println!(
        "Total number of utxos bigger than collateral minimum: {}",
        utxo_info.bigger_than_min_counter
    );
    println!(
        "Total number of utxos older than collateral coinage: {}",
        utxo_info.old_utxos_counter
    );

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
            .sorted_by_key(|&(_, (r, _))| std::cmp::Reverse(r))
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

pub fn get_transaction(addr: SocketAddr, hash: String) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let request = format!(
        r#"{{"jsonrpc": "2.0","method": "getTransaction", "params": [{:?}], "id": "1"}}"#,
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

#[allow(clippy::too_many_arguments)]
pub fn send_vtt(
    addr: SocketAddr,
    pkh: Option<PublicKeyHash>,
    value: u64,
    size: Option<u64>,
    fee: u64,
    time_lock: u64,
    sorted_bigger: Option<bool>,
    dry_run: bool,
) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;

    let size = size.unwrap_or(value);
    if value / size > 1000 {
        bail!("This transaction is creating more than 1000 outputs and may not be accepted by the miners");
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

    let mut vt_outputs = vec![];
    let mut value = value;
    while value >= 2 * size {
        value -= size;
        vt_outputs.push(ValueTransferOutput {
            pkh,
            value: size,
            time_lock,
        })
    }

    vt_outputs.push(ValueTransferOutput {
        pkh,
        value,
        time_lock,
    });

    let utxo_strategy = match sorted_bigger {
        Some(true) => UtxoSelectionStrategy::BigFirst,
        Some(false) => UtxoSelectionStrategy::SmallFirst,
        None => UtxoSelectionStrategy::Random,
    };

    let params = BuildVtt {
        vto: vt_outputs,
        fee,
        utxo_strategy,
    };

    let request = format!(
        r#"{{"jsonrpc": "2.0","method": "sendValue", "params": {}, "id": "1"}}"#,
        serde_json::to_string(&params)?
    );
    if dry_run {
        println!("{}", request);
    } else {
        let response = send_request(&mut stream, &request)?;
        println!("{}", response);
    }
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

    log::debug!("{}", serde_json::to_string(&dr)?);

    validate_data_request_output(&dr)?;
    validate_rad_request(&dr.data_request)?;

    // Is the data request serialized correctly?
    // Check that serializing the deserialized struct results in exactly the same bytes
    let witnet_dr_bytes = dr.to_pb_bytes()?;

    if dr_bytes != witnet_dr_bytes {
        log::warn!("Data request uses an invalid serialization, will be ignored.\nINPUT BYTES: {:02x?}\nWIT DR BYTES: {:02x?}",
              dr_bytes, witnet_dr_bytes
        );
        log::warn!(
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

pub fn master_key_export(
    addr: SocketAddr,
    write_to_path: Option<&Path>,
) -> Result<(), failure::Error> {
    let request = r#"{"jsonrpc": "2.0","method":"masterKeyExport","id": "1"}"#;
    let mut stream = start_client(addr)?;
    let response = send_request(&mut stream, &request)?;

    match parse_response(&response) {
        Ok(private_key_slip32) => {
            let private_key_slip32: String = private_key_slip32;
            let private_key = ExtendedSK::from_slip32(&private_key_slip32).unwrap().0;
            let public_key = ExtendedPK::from_secret_key(&CryptoEngine::new(), &private_key);
            let pkh = PublicKey::from(public_key.key).pkh();
            if let Some(base_path) = write_to_path {
                let path = base_path.join(format!("private_key_{}.txt", pkh));
                let mut file = create_credentials_file(&path)?;
                file.write_all(format!("{}\n", private_key_slip32).as_bytes())?;
                let full_path = Path::new(&path);
                println!(
                    "Private key written to {}",
                    full_path.canonicalize()?.as_path().display()
                );
            } else {
                println!("Private key for pkh {}:\n{}", pkh, private_key_slip32);
            }
        }
        Err(error) => {
            println!("{}", error);
        }
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct DataRequestTransactionInfo {
    data_request_tx_hash: String,
    data_request_output: DataRequestOutput,
    data_request_creator_pkh: String,
    block_hash_data_request_tx: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    block_hash_tally_tx: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data_request_state: Option<DataRequestState>,
    // [(pkh, reveal, reward_value)]
    #[serde(skip_serializing_if = "Option::is_none")]
    reveals: Option<Vec<(String, String, String)>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tally: Option<String>,
    #[serde(skip)]
    print_data_request: bool,
}

#[derive(Debug, Serialize)]
struct DataRequestState {
    stage: String,
    current_commit_round: u16,
    current_reveal_round: u16,
}

impl fmt::Display for DataRequestTransactionInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Report for data request {}:",
            White.bold().paint(&self.data_request_tx_hash)
        )?;

        if self.print_data_request {
            writeln!(
                f,
                "data_request_output: {}",
                serde_json::to_string_pretty(&self.data_request_output).unwrap()
            )?;
        }

        if self.block_hash_data_request_tx == "pending" {
            writeln!(
                f,
                "Deployed by {}, not yet included in any block",
                self.data_request_creator_pkh
            )?;
        } else {
            writeln!(
                f,
                "Deployed in block {} by {}",
                Purple.bold().paint(&self.block_hash_data_request_tx),
                self.data_request_creator_pkh
            )?;
            let data_request_state = self.data_request_state.as_ref().unwrap();
            let num_commits = self.reveals.as_ref().unwrap().len();
            let num_reveals = self
                .reveals
                .as_ref()
                .unwrap()
                .iter()
                .filter_map(
                    |(_pkh, reveal, _honest)| {
                        if reveal.is_empty() {
                            None
                        } else {
                            Some(())
                        }
                    },
                )
                .count();
            if data_request_state.stage == "FINISHED" {
                writeln!(
                    f,
                    "{} with {} commits and {} reveals",
                    White.bold().paint(&data_request_state.stage),
                    num_commits,
                    num_reveals,
                )?;
            } else {
                writeln!(
                    f,
                    "In {} stage with {} commits and {} reveals",
                    White.bold().paint(&data_request_state.stage),
                    num_commits,
                    num_reveals,
                )?;
            }
            writeln!(
                f,
                "Commit rounds: {}/{}",
                data_request_state.current_commit_round,
                1 + self.data_request_output.extra_commit_rounds
            )?;
            writeln!(
                f,
                "Reveal rounds: {}/{}",
                data_request_state.current_reveal_round,
                1 + self.data_request_output.extra_reveal_rounds
            )?;
        }

        if let Some(reveals) = &self.reveals {
            let data_request_state = self.data_request_state.as_ref().unwrap();
            if data_request_state.stage == "COMMIT" {
                writeln!(
                    f,
                    "Commits:{}",
                    if reveals.is_empty() {
                        " (no commits)"
                    } else {
                        ""
                    }
                )?;
            } else {
                writeln!(
                    f,
                    "Reveals:{}",
                    if reveals.is_empty() {
                        " (no reveals)"
                    } else {
                        ""
                    }
                )?;
            }
            for (pkh, reveal, reward) in reveals {
                let reveal_str = if reveal.is_empty() {
                    "No reveal"
                } else {
                    reveal
                };

                match reward.chars().next() {
                    Some('+') => {
                        writeln!(
                            f,
                            "    [Rewarded ] {}: {}",
                            pkh,
                            Yellow.bold().paint(reveal_str)
                        )?;
                    }
                    Some('-') => {
                        writeln!(
                            f,
                            "    {} {}: {}",
                            Red.bold().paint("[Penalized]"),
                            Red.bold().paint(pkh),
                            Yellow.bold().paint(reveal_str)
                        )?;
                    }
                    _ => {
                        writeln!(f, "    {}: {}", pkh, Yellow.bold().paint(reveal_str))?;
                    }
                }
            }
        } else {
            writeln!(f, "No reveals yet")?;
        }
        if let Some(tally) = &self.tally {
            writeln!(f, "Tally: {}", Yellow.bold().paint(tally))?;
        }

        Ok(())
    }
}

pub fn data_request_report(
    addr: SocketAddr,
    hash: String,
    json: bool,
    print_data_request: bool,
) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let request = format!(
        r#"{{"jsonrpc": "2.0","method": "getTransaction", "params": [{:?}], "id": "1"}}"#,
        hash,
    );
    let response = send_request(&mut stream, &request)?;
    let transaction: GetTransactionOutput = parse_response(&response)?;

    let data_request_transaction_block_hash = transaction.block_hash.clone();
    let transaction_block_hash = if transaction.block_hash == "pending" {
        None
    } else {
        Some(transaction.block_hash)
    };
    let dr_tx = if let Transaction::DataRequest(dr_tx) = transaction.transaction {
        dr_tx
    } else {
        bail!("This is not a data request transaction");
    };

    let dr_output = dr_tx.body.dr_output;
    let dr_creator_pkh = dr_tx.signatures[0].public_key.pkh();

    let (data_request_state, reveals, tally, block_hash_tally_tx) = if transaction_block_hash
        .is_none()
    {
        (None, None, None, None)
    } else {
        let request = format!(
            r#"{{"jsonrpc": "2.0","method": "dataRequestReport", "params": [{:?}], "id": "1"}}"#,
            hash,
        );
        let response = send_request(&mut stream, &request)?;
        let dr_info: DataRequestInfo = parse_response(&response)?;

        let data_request_state = DataRequestState {
            stage: dr_info
                .current_stage
                .map(|x| format!("{:?}", x))
                .unwrap_or_else(|| "FINISHED".to_string()),
            current_commit_round: dr_info.current_commit_round,
            current_reveal_round: dr_info.current_reveal_round,
        };

        let mut reveals = vec![];
        for (pkh, reveal_transaction) in &dr_info.reveals {
            let reveal_radon_types =
                RadonTypes::try_from(reveal_transaction.body.reveal.as_slice())?;
            reveals.push((*pkh, Some(reveal_radon_types)));
        }
        for pkh in dr_info.commits.keys() {
            if !reveals.iter().any(|(reveal_pkh, _)| reveal_pkh == pkh) {
                reveals.push((*pkh, None));
            }
        }
        // Sort reveal list by pkh
        reveals.sort_unstable_by_key(|(pkh, _)| *pkh);
        let reveals = reveals;

        let tally = dr_info
            .tally
            .as_ref()
            .map(|t| RadonTypes::try_from(t.tally.as_slice()))
            .transpose()?;

        (
            Some(data_request_state),
            Some(
                reveals
                    .into_iter()
                    .map(|(pkh, reveal)| {
                        let honest = match dr_info.tally.as_ref() {
                            None => format!(""),
                            Some(tally) => {
                                if tally.slashed_witnesses.contains(&pkh) {
                                    let reward = 0;

                                    format!("-{}", reward)
                                } else {
                                    let reward = tally
                                        .outputs
                                        .iter()
                                        .find(|vto| vto.pkh == pkh)
                                        .map(|vto| vto.value)
                                        .unwrap();

                                    // Note: the collateral is included in the reward
                                    format!("+{}", reward)
                                }
                            }
                        };
                        (
                            pkh.to_string(),
                            reveal.map(|x| x.to_string()).unwrap_or_default(),
                            honest,
                        )
                    })
                    .collect(),
            ),
            tally.map(|x| x.to_string()),
            dr_info.block_hash_tally_tx.map(|x| x.to_string()),
        )
    };

    let dr_info = DataRequestTransactionInfo {
        data_request_tx_hash: hash,
        data_request_output: dr_output,
        data_request_creator_pkh: dr_creator_pkh.to_string(),
        block_hash_data_request_tx: data_request_transaction_block_hash,
        block_hash_tally_tx,
        data_request_state,
        reveals,
        tally,
        print_data_request,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&dr_info)?);
    } else {
        // dr_info already ends with a newline, no need to println
        print!("{}", dr_info);
    }

    Ok(())
}

pub fn get_peers(addr: SocketAddr) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let request = r#"{"jsonrpc": "2.0","method": "peers", "id": "1"}"#;
    let response = send_request(&mut stream, &request)?;
    let peers: PeersResult = parse_response(&response)?;

    if peers.is_empty() {
        println!("No peers connected");
        return Ok(());
    }

    let mut table = Table::new();
    table.set_format(*prettytable::format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!["Address", "Type"]);
    for AddrType { address, type_ } in peers {
        table.add_row(row![address, type_]);
    }
    table.printstd();

    Ok(())
}

pub fn get_known_peers(addr: SocketAddr) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let request = r#"{"jsonrpc": "2.0","method": "knownPeers", "id": "1"}"#;
    let response = send_request(&mut stream, &request)?;
    let peers: PeersResult = parse_response(&response)?;

    if peers.is_empty() {
        println!("No known peers");
        return Ok(());
    }

    let mut table = Table::new();
    table.set_format(*prettytable::format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!["Address", "Type"]);
    for AddrType { address, type_ } in peers {
        table.add_row(row![address, type_]);
    }
    table.printstd();

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
