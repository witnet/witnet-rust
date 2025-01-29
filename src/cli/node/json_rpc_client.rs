use std::{
    cmp::Reverse,
    collections::{BTreeSet, HashMap, HashSet},
    convert::TryFrom,
    fmt,
    fs::File,
    io::{self, BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    str::FromStr,
};

use ansi_term::Color::{Purple, Red, White, Yellow};
use failure::{bail, Fail};
use itertools::Itertools;
use num_format::{Locale, ToFormattedString};
use prettytable::{row, Cell, Row, Table};
use qrcode::render::unicode;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use witnet_config::defaults::PSEUDO_CONSENSUS_CONSTANTS_WIP0022_REWARD_COLLATERAL_RATIO;
use witnet_crypto::{
    hash::calculate_sha256,
    key::{ExtendedPK, ExtendedSK},
};
use witnet_data_structures::{
    capabilities::{Capability, ALL_CAPABILITIES},
    chain::{
        priority::{PrioritiesEstimate, Priority, PriorityEstimate, TimeToBlock},
        tapi::{current_active_wips, ActiveWips},
        Block, ConsensusConstants, DataRequestInfo, DataRequestOutput, Environment, Epoch,
        Hashable, KeyedSignature, NodeStats, OutputPointer, PublicKey, PublicKeyHash, StateMachine,
        SupplyInfo, SyncStatus, ValueTransferOutput,
    },
    fee::Fee,
    get_environment,
    proto::{
        versioning::{ProtocolInfo, ProtocolVersion},
        ProtobufConvert,
    },
    staking::prelude::StakeEntry,
    transaction::{
        DRTransaction, StakeTransaction, Transaction, UnstakeTransaction, VTTransaction,
    },
    transaction_factory::NodeBalance,
    types::SequentialId,
    utxo_pool::{UtxoInfo, UtxoSelectionStrategy},
    wit::{Wit, WIT_DECIMAL_PLACES},
};
use witnet_node::actors::{
    chain_manager::run_dr_locally,
    json_rpc::api::{
        AddrType, GetBlockChainParams, GetTransactionOutput, PeersResult, QueryPowersParams,
        QueryPowersRecord,
    },
    messages::{
        AuthorizeStake, BuildDrt, BuildStakeParams, BuildStakeResponse, BuildUnstakeParams,
        BuildVtt, GetBalanceTarget, GetReputationResult, MagicEither, QueryStakes,
        QueryStakesFilter, QueryStakesLimits, SignalingInfo, StakeAuthorization,
    },
};
use witnet_rad::types::RadonTypes;
use witnet_util::{files::create_private_file, timestamp::pretty_print};
use witnet_validations::validations::{
    run_tally_panic_safe, validate_data_request_output, validate_rad_request,
};

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

// Get integer part of `nanowits / 10^9`: number of whole wits
fn whole_wits(nanowits: u64) -> u64 {
    Wit::wits_and_nanowits(Wit::from_nanowits(nanowits)).0
}

#[allow(
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
pub fn get_supply_info(addr: SocketAddr) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;

    let request = r#"{"jsonrpc": "2.0","method": "getSupplyInfo", "id": "1"}"#;
    let response = send_request(&mut stream, request)?;
    let supply_info = parse_response::<SupplyInfo>(&response)?;

    log::info!("{:?}", supply_info);

    println!(
        "\nSupply info at {} (epoch {}):\n",
        pretty_print(supply_info.current_time as i64, 0),
        supply_info.epoch
    );

    let block_rewards_wit = whole_wits(supply_info.blocks_minted_reward);
    let block_rewards_missing_wit = whole_wits(supply_info.blocks_missing_reward);
    let collateralized_data_requests_total_wit = whole_wits(supply_info.locked_wits_by_requests);
    let current_supply =
        whole_wits(supply_info.current_unlocked_supply + supply_info.locked_wits_by_requests);
    let locked_supply = whole_wits(supply_info.current_locked_supply);
    let total_supply = whole_wits(supply_info.maximum_supply - supply_info.blocks_missing_reward);
    let expected_total_supply = whole_wits(supply_info.maximum_supply);

    let mut supply_table = Table::new();
    supply_table.set_format(*prettytable::format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    supply_table.set_titles(row!["Supply type", r->"Total WITs"]);
    supply_table.add_row(row![
        "Temporarily locked in data requests".to_string(),
        r->collateralized_data_requests_total_wit.to_formatted_string(&Locale::en)
    ]);
    supply_table.add_row(row![
        "Unlocked supply".to_string(),
        r->current_supply.to_formatted_string(&Locale::en)
    ]);
    supply_table.add_row(row![
        "Locked supply".to_string(),
        r->locked_supply.to_formatted_string(&Locale::en)
    ]);
    supply_table.add_row(row![
        "Circulating supply".to_string(),
        r->(current_supply + locked_supply).to_formatted_string(&Locale::en)
    ]);
    supply_table.add_row(row![
        "Actual maximum supply".to_string(),
        r->total_supply.to_formatted_string(&Locale::en)
    ]);
    supply_table.add_row(row![
        "Expected maximum supply".to_string(),
        r->expected_total_supply.to_formatted_string(&Locale::en)
    ]);
    supply_table.printstd();
    println!();

    let mut blocks_table = Table::new();
    blocks_table.set_format(*prettytable::format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    blocks_table.set_titles(row!["Blocks", r->"Amount", r->"Total WITs"]);
    blocks_table.add_row(row![
        "Minted".to_string(),
        r->supply_info.blocks_minted.to_formatted_string(&Locale::en),
        r->block_rewards_wit.to_formatted_string(&Locale::en)
    ]);
    blocks_table.add_row(row![
        "Reverted".to_string(),
        r->supply_info.blocks_missing.to_formatted_string(&Locale::en),
        r->block_rewards_missing_wit.to_formatted_string(&Locale::en)
    ]);
    blocks_table.add_row(row![
        "Expected".to_string(),
        r->(supply_info.blocks_minted + supply_info.blocks_missing).to_formatted_string(&Locale::en),
        r->(block_rewards_wit + block_rewards_missing_wit).to_formatted_string(&Locale::en)
    ]);
    blocks_table.printstd();

    println!();
    println!(
        "{}% of circulating supply is locked.",
        ((locked_supply as f64 / (current_supply + locked_supply) as f64) * 100.0).round() as u8
    );
    println!(
        "{}% of all blocks so far have been reverted.",
        ((block_rewards_missing_wit as f64
            / (block_rewards_wit + block_rewards_missing_wit) as f64)
            * 100.0)
            .round() as u8
    );
    println!("For more information about block rewards and halvings, see:\nhttps://github.com/witnet/WIPs/blob/master/wip-0003.md");

    Ok(())
}

pub fn get_balance(
    addr: SocketAddr,
    target: GetBalanceTarget,
    simple: bool,
) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;

    if let GetBalanceTarget::Own = target {
        log::info!("No pkh specified, will default to node pkh");
        let request = r#"{"jsonrpc": "2.0","method": "getPkh", "id": "1"}"#;
        let response = send_request(&mut stream, request)?;
        let node_pkh = parse_response::<PublicKeyHash>(&response)?;
        log::info!("Node pkh: {}", node_pkh);
    }

    let request = format!(
        r#"{{"jsonrpc": "2.0","method": "getBalance", "params": [{}, {}], "id": "1"}}"#,
        serde_json::to_string(&target).unwrap(),
        serde_json::to_string(&simple).unwrap(),
    );
    log::info!("{}", request);
    let response = send_request(&mut stream, &request)?;
    log::info!("{}", response);

    let balances = parse_response::<NodeBalance>(&response)?;
    let list: Vec<_> = match balances {
        one @ NodeBalance::One { .. } => vec![(None, one)].into_iter().collect(),
        NodeBalance::Many(many) => many
            .into_iter()
            .map(|(key, val)| (Some(key), val))
            .collect(),
    };

    for (address, balance) in list {
        if let Some(address) = address {
            println!(
                "== {} ==",
                address.bech32(witnet_data_structures::get_environment())
            );
        }

        if simple {
            println!("Balance:   {} wits", balance.get_total().unwrap());
        } else {
            let total = balance.get_total().unwrap();
            let confirmed = balance.get_confirmed().unwrap_or(total);
            let pending = wit_difference_to_string(confirmed, total);
            println!(
                "Confirmed balance:   {} wits\n\
                 Pending balance:     {} wits",
                confirmed, pending
            );
        }
    }

    Ok(())
}

// Check if the pending balance is positive or negative
fn wit_difference_to_string(confirmed: Wit, total: Wit) -> String {
    if total >= confirmed {
        (total - confirmed).to_string()
    } else {
        let mut neg = String::from("-");
        neg.push_str(&(confirmed - total).to_string());
        neg
    }
}

pub fn get_pkh(addr: SocketAddr) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let request = r#"{"jsonrpc": "2.0","method": "getPkh", "id": "1"}"#;
    let response = send_request(&mut stream, request)?;
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
            let response = send_request(&mut stream, request)?;
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
    let mut utxo_sum = 0;

    let mut utxo_too_small_counter = 0;
    let mut utxo_too_small_sum = 0;

    let mut utxo_not_ready_counter = 0;
    let mut utxo_not_ready_sum = 0;

    let mut utxo_ready_counter = 0;
    let mut utxo_ready_sum = 0;

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
        .sorted_by_key(|um| (um.value, um.output_pointer))
    {
        let ready_for_collateral: bool = (utxo_metadata.value >= utxo_info.collateral_min)
            && utxo_metadata.utxo_mature
            && utxo_metadata.timelock == 0;

        if long {
            let value = Wit::from_nanowits(utxo_metadata.value).to_string();
            let time_lock = if utxo_metadata.timelock == 0 {
                "Ready".to_string()
            } else {
                pretty_print(utxo_metadata.timelock as i64, 0)
            };

            table.add_row(row![
                utxo_metadata.output_pointer.to_string(),
                value,
                time_lock,
                ready_for_collateral.to_string()
            ]);
        }

        utxo_sum += utxo_metadata.value;
        // Utxo bigger than collateral minimum, no timelock and mature
        if ready_for_collateral {
            utxo_ready_counter += 1;
            utxo_ready_sum += utxo_metadata.value;
        // Utxo smaller than collateral_min, can never be collateralized (until joined)
        } else if utxo_metadata.value < utxo_info.collateral_min {
            utxo_too_small_counter += 1;
            utxo_too_small_sum += utxo_metadata.value;
        // Utxo with a timelock enabled or utxo bigger than collateral minimum, no timelock but not mature
        } else {
            utxo_not_ready_counter += 1;
            utxo_not_ready_sum += utxo_metadata.value;
        }
    }

    if long {
        table.printstd();
        println!("-----------------------");
    }

    let mut utxos_table = Table::new();
    utxos_table.set_format(*prettytable::format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    utxos_table.set_titles(row!["Utxos", "Number", "Value (in wits)"]);
    utxos_table.add_row(row![
        "Total utxos".to_string(),
        utxos_len,
        Wit::from_nanowits(utxo_sum).to_string()
    ]);
    utxos_table.add_row(row![
        "Utxos smaller than collateral minimum".to_string(),
        utxo_too_small_counter,
        Wit::from_nanowits(utxo_too_small_sum).to_string()
    ]);
    utxos_table.add_row(row![
        "Utxos bigger than collateral minimum".to_string(),
        (utxos_len - utxo_too_small_counter),
        Wit::from_nanowits(utxo_sum - utxo_too_small_sum).to_string()
    ]);
    utxos_table.add_row(row![
        "Utxos bigger than and ready for collateral".to_string(),
        utxo_ready_counter,
        Wit::from_nanowits(utxo_ready_sum).to_string()
    ]);
    utxos_table.add_row(row![
        "Utxos bigger than and not ready for collateral".to_string(),
        utxo_not_ready_counter,
        Wit::from_nanowits(utxo_not_ready_sum).to_string()
    ]);
    utxos_table.printstd();

    Ok(())
}

#[allow(clippy::cast_precision_loss)]
pub fn get_reputation(
    addr: SocketAddr,
    opt_pkh: Option<PublicKeyHash>,
    all: bool,
) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;

    let request = if all {
        r#"{"jsonrpc": "2.0","method": "getReputationAll", "id": "1"}"#.to_string()
    } else {
        let pkh = match opt_pkh {
            Some(pkh) => pkh,
            None => {
                log::info!("No pkh specified, will default to node pkh");
                let request = r#"{"jsonrpc": "2.0","method": "getPkh", "id": "1"}"#;
                let response = send_request(&mut stream, request)?;
                let node_pkh = parse_response::<PublicKeyHash>(&response)?;
                log::info!("Node pkh: {}", node_pkh);

                node_pkh
            }
        };

        format!(
            r#"{{"jsonrpc": "2.0","method": "getReputation", "params": [{}], "id": "1"}}"#,
            serde_json::to_string(&pkh)?,
        )
    };
    let response = send_request(&mut stream, &request)?;
    let res = parse_response::<GetReputationResult>(&response)?;

    if res.stats.is_empty() {
        println!("No identities have reputation yet");
    }
    for (pkh, rep_stats) in res.stats.into_iter().sorted_by_key(|(_, rep_stats)| {
        std::cmp::Reverse((rep_stats.reputation.0, rep_stats.eligibility))
    }) {
        let eligibility = f64::from(rep_stats.eligibility) / res.total_reputation as f64;
        let active = if rep_stats.is_active { 'A' } else { ' ' };
        if rep_stats.is_active || !all {
            println!(
                "    [{}] {} -> Reputation: {}, Eligibility: {:.6}%",
                active,
                pkh,
                rep_stats.reputation.0,
                eligibility * 100_f64
            );
        } else {
            println!(
                "    [{}] {} -> Reputation: {}",
                active, pkh, rep_stats.reputation.0
            );
        }
    }

    Ok(())
}

pub fn get_miners(addr: SocketAddr, start: i64, end: i64, csv: bool) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let params = GetBlockChainParams {
        epoch: start,
        limit: end,
    };
    let response = send_request(
        &mut stream,
        &format!(
            r#"{{"jsonrpc": "2.0","method": "getBlockChain", "params": {}, "id": 1}}"#,
            serde_json::to_string(&params).unwrap()
        ),
    )?;
    log::info!("{}", response);
    let block_chain: ResponseBlockChain<'_> = parse_response(&response)?;
    let mut hm = HashMap::new();

    if csv {
        println!("Block number;Block hash;Miner hash")
    } else {
        println!("Blockchain:");
    }
    for (epoch, hash) in block_chain {
        let request = format!(
            r#"{{"jsonrpc": "2.0","method": "getBlock", "params": [{:?}], "id": "1"}}"#,
            hash,
        );
        let response = send_request(&mut stream, &request)?;
        let block: Block = parse_response(&response)?;
        let miner_hash = block.block_sig.public_key.pkh().to_string();

        if csv {
            println!("{};{};{}", epoch, hash, miner_hash);
        } else {
            println!(
                "Block for epoch #{} had digest {} ans was mined by {}",
                epoch, hash, miner_hash
            );
        }

        *hm.entry(miner_hash).or_insert(0) += 1;
    }

    let mut scoreboard: Vec<(String, i32)> = hm.into_iter().collect();
    scoreboard.sort_by_key(|(m, _n)| m.clone());
    if csv {
        println!("\nMiner address;Mined blocks count");
    } else {
        println!("\nScoreboard:");
    }
    for (miner, n) in scoreboard.iter() {
        if csv {
            println!("{};{}", miner, n);
        } else {
            println!("{} has mined {} blocks", miner, n);
        }
    }

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
    //let response = send_request(&mut stream, request)?;
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
    fee: Option<Fee>,
    time_lock: u64,
    sorted_bigger: Option<bool>,
    dry_run: bool,
) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let mut id = SequentialId::initialize(1u8);

    let size = size.unwrap_or(value);
    if value / size > 1000 {
        bail!("This transaction is creating more than 1000 outputs and may not be accepted by the miners");
    }

    // Prepare for fee estimation if no fee value was specified
    let (fee, estimate) = unwrap_fee_or_estimate_priority(fee, &mut stream, &mut id)?;

    let pkh = match pkh {
        Some(pkh) => pkh,
        None => {
            log::info!("No pkh specified, will default to node pkh");
            let (node_pkh, ..) =
                issue_method("getPkh", None::<serde_json::Value>, &mut stream, id.next())?;
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
        Some(true) => UtxoSelectionStrategy::BigFirst { from: None },
        Some(false) => UtxoSelectionStrategy::SmallFirst { from: None },
        None => UtxoSelectionStrategy::Random { from: None },
    };

    let mut params = BuildVtt {
        vto: vt_outputs,
        fee,
        utxo_strategy,
        dry_run,
    };

    // If no fee was specified, we first need to do a dry run for each of the priority tiers to
    // find out the actual transaction weight (as different priorities will affect the number
    // of inputs being used, and thus also the weight).
    if let Some(PrioritiesEstimate {
        vtt_stinky,
        vtt_low,
        vtt_medium,
        vtt_high,
        vtt_opulent,
        ..
    }) = estimate
    {
        let priorities = vec![
            (vtt_stinky, "Stinky"),
            (vtt_low, "Low"),
            (vtt_medium, "Medium"),
            (vtt_high, "High"),
            (vtt_opulent, "Opulent"),
        ];
        let mut estimates = vec![];
        let mut fee;

        // Iterative algorithm for transaction weight discovery. It calculates the fees for this
        // transaction assuming that it has the minimum weight, and then repeats the estimation
        // using the actual weight of the latest created transaction, until the weight stabilizes
        // or after 5 rounds.
        for (
            PriorityEstimate {
                priority,
                time_to_block,
            },
            label,
        ) in priorities
        {
            // The minimum VTT size is 169 weight units as per WIP-0007
            let mut weight = 169u32;
            let mut rounds = 0u8;
            // Iterative algorithm for weight discovery
            loop {
                // Calculate fee for current priority and weight
                fee = Fee::absolute_from_wit(priority.derive_fee_wit(weight));

                // Create and dry run a VTT transaction using that fee
                let dry_params = BuildVtt {
                    fee,
                    dry_run: true,
                    ..params.clone()
                };
                let (dry_vtt, ..): (VTTransaction, _) =
                    issue_method("sendValue", Some(dry_params), &mut stream, id.next())?;
                let dry_weight = dry_vtt.weight();

                // We retry up to 5 times, or until the weight is stable
                if rounds > 5 || dry_weight == weight {
                    break;
                }

                weight = dry_weight;
                rounds += 1;
            }

            estimates.push((label, priority, fee, time_to_block));
        }

        // We are ready to compose the params for the actual transaction.
        params.fee = prompt_user_for_priority_selection(estimates)?;
    }

    // Finally ask the node to create the transaction with the chosen fee.
    let (_vtt, (request, response)): (VTTransaction, _) =
        issue_method("sendValue", Some(params), &mut stream, id.next())?;

    // On dry run mode, print the request, otherwise, print the response.
    // This is kept like this strictly for backwards compatibility.
    // TODO: wouldn't it be better to always print the response or both?
    if dry_run {
        println!("{}", request);
    } else {
        println!("{}", response);
    }

    Ok(())
}

fn deserialize_and_validate_hex_dr(
    hex_bytes: String,
    collateral_minimum: u64,
    required_reward_collateral_ratio: u64,
) -> Result<DataRequestOutput, failure::Error> {
    let dr_bytes = hex::decode(hex_bytes)?;

    let dr: DataRequestOutput = ProtobufConvert::from_pb_bytes(&dr_bytes)?;

    log::debug!("{}", serde_json::to_string(&dr)?);

    validate_data_request_output(
        &dr,
        collateral_minimum,
        required_reward_collateral_ratio,
        &current_active_wips(),
    )?;
    validate_rad_request(&dr.data_request, &current_active_wips())?;

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
    fee: Option<Fee>,
    dry_run: bool,
) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let request = r#"{"jsonrpc": "2.0","method": "getConsensusConstants", "id": "1"}"#;
    let response = send_request(&mut stream, request)?;
    let consensus_constants: ConsensusConstants = parse_response(&response)?;
    let required_reward_collateral_ratio =
        PSEUDO_CONSENSUS_CONSTANTS_WIP0022_REWARD_COLLATERAL_RATIO;
    let dro = deserialize_and_validate_hex_dr(
        hex_bytes,
        consensus_constants.collateral_minimum,
        required_reward_collateral_ratio,
    )?;
    let mut id = SequentialId::initialize(1u8);

    if dry_run {
        // TODO: this is not a proper dry run of this method but rather a local execution. Shall we
        //  have a different method or flag for this, and in case of dry_run, return the signed
        //  transaction?
        let tally_result = run_dr_locally(&dro)?;

        println!("Request run locally with Tally result: {}", tally_result);
    } else {
        // Prepare for fee estimation if no fee value was specified
        let (fee, estimate) = unwrap_fee_or_estimate_priority(fee, &mut stream, &mut id)?;

        let mut params = BuildDrt { dro, fee, dry_run };

        // If no fee was specified, we first need to do a dry run for each of the priority tiers to
        // find out the actual transaction weight (as different priorities will affect the number
        // of inputs being used, and thus also the weight).
        if let Some(PrioritiesEstimate {
            drt_stinky,
            drt_low,
            drt_medium,
            drt_high,
            drt_opulent,
            ..
        }) = estimate
        {
            let priorities = vec![
                (drt_stinky, "Stinky"),
                (drt_low, "Low"),
                (drt_medium, "Medium"),
                (drt_high, "High"),
                (drt_opulent, "Opulent"),
            ];
            let mut estimates = vec![];
            let mut fee;

            // Iterative algorithm for transaction weight discovery. It calculates the fees for this
            // transaction assuming that it has the minimum weight, and then repeats the estimation
            // using the actual weight of the latest created transaction, until the weight stabilizes
            // or after 5 rounds.
            for (
                PriorityEstimate {
                    priority,
                    time_to_block,
                },
                label,
            ) in priorities
            {
                // The minimum DRT size is 400 weight units as per WIP-0007
                let mut weight = 400u32;
                let mut rounds = 0u8;
                // Iterative algorithm for weight discovery
                loop {
                    // Calculate fee for current priority and weight
                    fee = Fee::absolute_from_wit(priority.derive_fee_wit(weight));

                    // Create and dry run a VTT transaction using that fee
                    let dry_params = BuildDrt {
                        fee,
                        dry_run: true,
                        ..params.clone()
                    };
                    let (dry_drt, ..): (DRTransaction, _) =
                        issue_method("sendRequest", Some(dry_params), &mut stream, id.next())?;
                    let dry_weight = dry_drt.weight();

                    // We retry up to 5 times, or until the weight is stable
                    if rounds > 5 || dry_weight == weight {
                        break;
                    }

                    weight = dry_weight;
                    rounds += 1;
                }

                estimates.push((label, priority, fee, time_to_block));
            }

            // We are ready to compose the params for the actual transaction.
            params.fee = prompt_user_for_priority_selection(estimates)?;
        }

        let (_, (_, response)): (DRTransaction, _) =
            issue_method("sendRequest", Some(params), &mut stream, id.next())?;

        println!("{}", response);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn send_st(
    addr: SocketAddr,
    value: u64,
    authorization: String,
    withdrawer: MagicEither<String, PublicKeyHash>,
    fee: Option<Fee>,
    sorted_bigger: Option<bool>,
    requires_confirmation: Option<bool>,
    dry_run: bool,
) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let mut id = SequentialId::initialize(1u8);

    // Prepare for fee estimation if no fee value was specified
    let (fee, estimate) = unwrap_fee_or_estimate_priority(fee, &mut stream, &mut id)?;

    let utxo_strategy = match sorted_bigger {
        Some(true) => UtxoSelectionStrategy::BigFirst { from: None },
        Some(false) => UtxoSelectionStrategy::SmallFirst { from: None },
        None => UtxoSelectionStrategy::Random { from: None },
    };

    let mut build_stake_params = BuildStakeParams {
        authorization: MagicEither::Left(authorization.clone()),
        withdrawer,
        value,
        fee,
        utxo_strategy,
        dry_run,
    };

    // If no fee was specified, we first need to do a dry run for each of the priority tiers to
    // find out the actual transaction weight (as different priorities will affect the number
    // of inputs being used, and thus also the weight).
    if let Some(PrioritiesEstimate {
        vtt_stinky,
        vtt_low,
        vtt_medium,
        vtt_high,
        vtt_opulent,
        ..
    }) = estimate
    {
        let priorities = vec![
            (vtt_stinky, "Stinky"),
            (vtt_low, "Low"),
            (vtt_medium, "Medium"),
            (vtt_high, "High"),
            (vtt_opulent, "Opulent"),
        ];
        let mut estimates = vec![];
        let mut fee;

        // Iterative algorithm for transaction weight discovery. It calculates the fees for this
        // transaction assuming that it has the minimum weight, and then repeats the estimation
        // using the actual weight of the latest created transaction, until the weight stabilizes
        // or after 5 rounds.
        for (
            PriorityEstimate {
                priority,
                time_to_block,
            },
            label,
        ) in priorities
        {
            // The minimum ST size is N*133+M*36+105` where `N` is the number of `inputs`, and `M`
            // is 0 or 1 depending on whether a `change` output is used
            let mut weight = 238u32;
            let mut rounds = 0u8;
            // Iterative algorithm for weight discovery
            loop {
                // Calculate fee for current priority and weight
                fee = Fee::absolute_from_wit(priority.derive_fee_wit(weight));

                // Create and dry run a Stake transaction using that fee
                let dry_params = BuildStakeParams {
                    fee,
                    dry_run: true,
                    ..build_stake_params.clone()
                };
                let (bsr, ..): (BuildStakeResponse, _) =
                    issue_method("stake", Some(dry_params), &mut stream, id.next())?;
                let dry_weight = bsr.transaction.weight();

                // We retry up to 5 times, or until the weight is stable
                if rounds > 5 || dry_weight == weight {
                    break;
                }

                weight = dry_weight;
                rounds += 1;
            }

            estimates.push((label, priority, fee, time_to_block));
        }

        // We are ready to compose the params for the actual transaction.
        build_stake_params.fee = prompt_user_for_priority_selection(estimates)?;
    }

    let params = BuildStakeParams {
        dry_run: true,
        ..build_stake_params.clone()
    };
    let (dry, _): (BuildStakeResponse, _) =
        issue_method("stake", Some(params), &mut stream, id.next())?;

    let validator_address = {
        let pkh_bytes =
            hex::decode(authorization.clone().chars().take(40).collect::<String>()).unwrap();
        PublicKeyHash::from_bytes(pkh_bytes.as_slice())?
    };
    if validator_address != dry.validator {
        bail!(
            "The validator derived from the authorization string ({}) \
            does not match the validator calculated using the specified withdrawer ({}). \
            Please verify that you are using the same withdrawer address that was used \
            to generate the authorization string.",
            validator_address,
            dry.validator.to_string(),
        );
    }

    let confirmation = if requires_confirmation.unwrap_or(true) {
        // Exactly what it says: shows all the facts about the staking transaction, and expects confirmation through
        // user input
        if prompt_user_for_stake_confirmation(&dry)? {
            Some(dry)
        } else {
            None
        }
    } else {
        Some(dry)
    };

    if let Some(dry) = confirmation {
        // Finally ask the node to create the transaction with the chosen fee.
        build_stake_params.dry_run = dry_run;
        let (st, (request, response)): (StakeTransaction, _) =
            issue_method("stake", Some(build_stake_params), &mut stream, id.next())?;

        println!("> {}", request);
        println!("< {}", response);

        let environment = get_environment();
        let value = Wit::from_nanowits(st.body.output.value).to_string();
        let staker = dry
            .staker
            .iter()
            .map(|pkh| pkh.bech32(environment))
            .collect::<HashSet<_>>()
            .iter()
            .join(",");
        let validator = dry.validator.bech32(environment);
        let withdrawer = dry.withdrawer.bech32(environment);

        println!("Congratulations! {} Wit have been staked by addresses {:?} onto validator {}, using {} as the withdrawal address.", value, staker, validator, withdrawer);
    } else {
        println!("The stake facts have not been confirmed. No stake transaction has been created.");
    }

    Ok(())
}

pub fn authorize_st(addr: SocketAddr, withdrawer: Option<String>) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let mut id = SequentialId::initialize(1u8);

    let params = AuthorizeStake { withdrawer };
    let (authorization, (_, _response)): (StakeAuthorization, _) =
        issue_method("authorizeStake", Some(params), &mut stream, id.next())?;

    let message = authorization.withdrawer.as_secp256k1_msg();

    let auth_string = {
        let validator_bytes: [u8; 20] = authorization
            .signature
            .public_key
            .pkh()
            .as_ref()
            .try_into()?;
        let signature_bytes: [u8; 65] = authorization
            .signature
            .to_recoverable_bytes(&message)
            .unwrap();
        hex::encode([&validator_bytes[..], &signature_bytes[..]].concat())
    };

    let auth_qr = qrcode::QrCode::new(&auth_string)?;
    let auth_ascii = auth_qr
        .render::<unicode::Dense1x2>()
        .quiet_zone(true)
        .dark_color(unicode::Dense1x2::Light)
        .light_color(unicode::Dense1x2::Dark)
        .build();

    println!(
        "Authorization code:\n{}\nQR code for myWitWallet:\n{}",
        auth_string, auth_ascii
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn send_ut(
    addr: SocketAddr,
    operator: MagicEither<String, PublicKeyHash>,
    value: u64,
    fee: u64,
    dry_run: bool,
) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let mut id = SequentialId::initialize(1u8);

    let build_unstake_params = BuildUnstakeParams {
        operator,
        value,
        fee,
        dry_run,
    };

    // Finally ask the node to create the transaction.
    let (_, (request, response)): (UnstakeTransaction, _) = issue_method(
        "unstake",
        Some(build_unstake_params),
        &mut stream,
        id.next(),
    )?;

    // On dry run mode, print the request, otherwise, print the response.
    // This is kept like this strictly for backwards compatibility.
    // TODO: wouldn't it be better to always print the response or both?
    if dry_run {
        println!("{}", request);
    } else {
        println!("{}", response);
    }

    Ok(())
}

pub fn master_key_export(
    addr: SocketAddr,
    write_to_path: Option<&Path>,
) -> Result<(), failure::Error> {
    let request = r#"{"jsonrpc": "2.0","method":"masterKeyExport","id": "1"}"#;
    let mut stream = start_client(addr)?;
    let response = send_request(&mut stream, request)?;

    match parse_response(&response) {
        Ok(private_key_slip32) => {
            let private_key_slip32: String = private_key_slip32;
            let private_key = ExtendedSK::from_slip32(&private_key_slip32).unwrap().0;
            let public_key = ExtendedPK::from_secret_key(&private_key);
            let pkh = PublicKey::from(public_key.key).pkh();
            if let Some(base_path) = write_to_path {
                let path = base_path.join(format!("private_key_{}.txt", pkh));
                let mut file = create_private_file(&path)?;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    local_tally: Option<String>,
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
                "Commit rounds: {}",
                data_request_state.current_commit_round,
            )?;
            writeln!(
                f,
                "Reveal rounds: {}",
                data_request_state.current_reveal_round,
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
                    // Neither positive or negative means that the collateral was returned to the
                    // witness, but it has not been rewarded. This happens when the witness
                    // committed an error but the consensus is not an error.
                    _ => {
                        if data_request_state.stage == "FINISHED" {
                            writeln!(
                                f,
                                "    [  Error  ] {}: {}",
                                pkh,
                                Yellow.bold().paint(reveal_str)
                            )?;
                        } else {
                            writeln!(f, "    {}: {}", pkh, Yellow.bold().paint(reveal_str))?;
                        }
                    }
                }
            }
        } else {
            writeln!(f, "No reveals yet")?;
        }
        if let Some(tally) = &self.tally {
            writeln!(f, "Tally: {}", Yellow.bold().paint(tally))?;
        }
        if let Some(local_tally) = &self.local_tally {
            writeln!(f, "Local tally: {}", Yellow.bold().paint(local_tally))?;
        }

        Ok(())
    }
}

pub fn data_request_report(
    addr: SocketAddr,
    hash: String,
    json: bool,
    print_data_request: bool,
    create_local_tally: bool,
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

    let request = r#"{"jsonrpc": "2.0","method": "protocol", "id": "1"}"#;
    let response = send_request(&mut stream, request)?;
    let protocol_info: Option<ProtocolInfo> = parse_response(&response)?;

    let version_at_epoch = if let Some(info) = protocol_info {
        match transaction.block_epoch {
            Some(epoch) => info.all_versions.version_for_epoch(epoch),
            None => ProtocolVersion::default(),
        }
    } else {
        ProtocolVersion::default()
    };

    let mut dr_output = dr_tx.body.dr_output;
    let dr_creator_pkh = dr_tx.signatures[0].public_key.pkh();

    // When collateral is set to 0, it is actually the default collateral
    // Get the consensus constants from to node to find out what is the default collateral
    if dr_output.collateral == 0 {
        let request = r#"{"jsonrpc": "2.0","method": "getConsensusConstants", "id": "1"}"#;
        let response = send_request(&mut stream, request)?;
        let consensus_constants: ConsensusConstants = parse_response(&response)?;
        dr_output.collateral = consensus_constants.collateral_minimum;
    }

    let (data_request_state, reveals, tally, local_tally, block_hash_tally_tx) =
        if transaction_block_hash.is_none() {
            (None, None, None, None, None)
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
            let mut reveal_txns = vec![];
            for (pkh, reveal_transaction) in &dr_info.reveals {
                let reveal_radon_types =
                    RadonTypes::try_from(reveal_transaction.body.reveal.as_slice())?;
                reveals.push((*pkh, Some(reveal_radon_types)));
                reveal_txns.push(reveal_transaction);
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

            let mut local_tally = None;

            if create_local_tally {
                // Run the tally stage locally. This can be useful if the result is a RadonError,
                // because it may report a better error message.

                // Get the activation epochs of the current active WIPs from the node
                let request = r#"{"jsonrpc": "2.0","method": "signalingInfo", "id": "1"}"#;
                let response = send_request(&mut stream, request)?;
                let signaling_info: SignalingInfo = parse_response(&response)?;

                // Get the tally block epoch from the tally block hash
                let request = format!(
                    r#"{{"jsonrpc": "2.0","method": "getBlock", "params": [{:?}], "id": "1"}}"#,
                    dr_info.block_hash_tally_tx.unwrap().to_string(),
                );
                let response = send_request(&mut stream, &request)?;
                let tally_block: Block = parse_response(&response)?;
                let tally_block_epoch = tally_block.block_header.beacon.checkpoint;

                // Run tally locally
                let active_wips = ActiveWips {
                    active_wips: signaling_info.active_upgrades,
                    block_epoch: tally_block_epoch,
                };
                let non_error_min = f64::from(dr_output.min_consensus_percentage) / 100.0;
                let commits_count = dr_info.commits.len();
                let report = run_tally_panic_safe(
                    &reveal_txns,
                    &dr_output.data_request.tally,
                    non_error_min,
                    commits_count,
                    &active_wips,
                    false,
                );

                local_tally = Some(report.into_inner());
            }

            (
                Some(data_request_state),
                Some(
                    reveals
                        .into_iter()
                        .map(|(pkh, reveal)| {
                            let honest = match dr_info.tally.as_ref() {
                                None => String::new(),
                                Some(tally) => {
                                    if tally.out_of_consensus.contains(&pkh)
                                        && !tally.error_committers.contains(&pkh)
                                    {
                                        format!("-{}", dr_output.collateral)
                                    } else {
                                        let reward = if version_at_epoch >= ProtocolVersion::V2_0 {
                                            dr_output.witness_reward + dr_output.collateral
                                        } else {
                                            tally
                                                .outputs
                                                .iter()
                                                .find(|vto| vto.pkh == pkh)
                                                .map(|vto| vto.value)
                                                .unwrap()
                                        };

                                        let reward = reward - dr_output.collateral;

                                        // Note: the collateral is not included in the reward
                                        if reward == 0 {
                                            "0".to_string()
                                        } else {
                                            format!("+{}", reward)
                                        }
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
                local_tally.map(|x| x.to_string()),
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
        local_tally,
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

pub fn search_requests(
    addr: SocketAddr,
    start: i64,
    end: i64,
    hex_dr_bytes: Option<String>,
    same_as_dr_tx: Option<String>,
) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;

    let expected_dr_output_bytes = match (hex_dr_bytes, same_as_dr_tx) {
        (Some(hex_dr_bytes), None) => {
            // Use dr_output_bytes from argument
            hex::decode(hex_dr_bytes)?
        }
        (None, Some(dr_tx_hash)) => {
            // Use dr_output_bytes from data request provided as argument
            let request = format!(
                r#"{{"jsonrpc": "2.0","method": "getTransaction", "params": [{:?}], "id": "1"}}"#,
                dr_tx_hash,
            );
            let response = send_request(&mut stream, &request)?;
            let transaction: GetTransactionOutput = parse_response(&response)?;

            let dr_tx = if let Transaction::DataRequest(dr_tx) = transaction.transaction {
                dr_tx
            } else {
                bail!("This is not a data request transaction");
            };

            let bytes = dr_tx.body.dr_output.to_pb_bytes()?;

            log::info!(
                "Searching for this dr_output_bytes: {}",
                hex::encode(&bytes)
            );

            bytes
        }
        _ => {
            bail!("Expected exactly 1 argument out of --hex-dr-bytes or --same-as-dr-tx")
        }
    };

    let params = GetBlockChainParams {
        epoch: start,
        limit: end,
    };
    let response = send_request(
        &mut stream,
        &format!(
            r#"{{"jsonrpc": "2.0","method": "getBlockChain", "params": {}, "id": 1}}"#,
            serde_json::to_string(&params).unwrap()
        ),
    )?;
    let block_chain: ResponseBlockChain<'_> = parse_response(&response)?;
    log::info!("Processing {} blocks", block_chain.len());

    for (_epoch, hash) in block_chain {
        let request = format!(
            r#"{{"jsonrpc": "2.0","method": "getBlock", "params": [{:?}], "id": "1"}}"#,
            hash,
        );
        let response = send_request(&mut stream, &request)?;
        let block: Block = parse_response(&response)?;

        for data_request in &block.txns.data_request_txns {
            let dr_output = &data_request.body.dr_output;
            let dr_output_bytes = dr_output.to_pb_bytes()?;
            if dr_output_bytes == expected_dr_output_bytes {
                let dr_hash = data_request.hash();
                println!("{}", dr_hash);
            }
        }
    }

    Ok(())
}

pub fn get_peers(addr: SocketAddr) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let request = r#"{"jsonrpc": "2.0","method": "peers", "id": "1"}"#;
    let response = send_request(&mut stream, request)?;
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
    let response = send_request(&mut stream, request)?;
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

pub fn get_node_stats(addr: SocketAddr) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let request = r#"{"jsonrpc": "2.0","method": "nodeStats", "id": "1"}"#;
    let response = send_request(&mut stream, request)?;
    let node_stats: NodeStats = parse_response(&response)?;

    println!(
        "Block mining stats:\n\
     - Proposed blocks: {}\n\
     - Blocks included in the block chain: {}\n\
    Data Request mining stats:\n\
     - Times with eligibility to mine a data request: {}\n\
     - Proposed commits: {}\n\
     - Accepted commits: {}\n\
     - Slashed commits: {}",
        node_stats.block_proposed_count,
        node_stats.block_mined_count,
        node_stats.dr_eligibility_count,
        node_stats.commits_proposed_count,
        node_stats.commits_count,
        node_stats.slashed_count
    );

    let request = r#"{"jsonrpc": "2.0","method": "syncStatus", "id": "1"}"#;
    let response = send_request(&mut stream, request)?;
    let sync_status: SyncStatus = parse_response(&response)?;

    if let Some(current_epoch) = sync_status.current_epoch {
        if sync_status.node_state == StateMachine::Synced {
            println!(
                "The node is synchronized and the current epoch is {}",
                current_epoch
            );
        } else {
            // Show progress log
            let mut percent_done_float =
                f64::from(sync_status.chain_beacon.checkpoint) / f64::from(current_epoch) * 100.0;

            // Never show 100% unless it's actually done
            if sync_status.chain_beacon.checkpoint != current_epoch && percent_done_float > 99.99 {
                percent_done_float = 99.99;
            }
            let percent_done_string = format!("{:.2}%", percent_done_float);
            let node_state = sync_status.node_state;

            println!(
                "Synchronization progress: {} ({:>6}/{:>6}), the current node state is {:?}",
                percent_done_string, sync_status.chain_beacon.checkpoint, current_epoch, node_state,
            );
        }
    } else {
        println!("The node is waiting for epoch 0");
    }

    Ok(())
}

pub fn get_protocol(addr: SocketAddr) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let request = r#"{"jsonrpc": "2.0","method": "protocol", "id": "1"}"#;
    let response = send_request(&mut stream, request)?;
    let protocol_info: Option<ProtocolInfo> = parse_response(&response)?;

    let version = if let Some(ProtocolInfo {
        current_version, ..
    }) = protocol_info
    {
        current_version.to_string()
    } else {
        format!(
            "unknown (assumed to be {}, but it could be older)",
            ProtocolVersion::V1_7
        )
    };

    println!(
        "Protocol Info:\n\
     - Current protocol version: {}",
        version
    );

    Ok(())
}

pub fn add_peers(addr: SocketAddr, peers: Vec<SocketAddr>) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    if peers.is_empty() {
        // If there were no peers as CLI arguments, read the addresses from stdin
        println!("No peer addresses specified in command line. Please enter the addresses:");
        let mut buf = String::new();
        let stdin = io::stdin();
        let mut stdin = stdin.lock();
        // Process stdin line by line, it's slower but this way we can keep adding peers one at a time
        loop {
            buf.clear();
            let count = stdin.read_line(&mut buf)?;
            // Exit on Ctrl-D
            if count == 0 {
                return Ok(());
            }

            let params: Vec<String> = buf
                .split(|c: char| {
                    // Split line by anything that is not an address: "[0-9]|\.|:"
                    // This allows us to accept any possible format, JSON, TOML, anything
                    !(c.is_numeric() || c == '.' || c == ':')
                })
                .filter_map(|addr| {
                    let addr: Option<SocketAddr> = addr.parse().ok();

                    addr
                })
                .map(|addr| addr.to_string())
                .collect();

            if params.is_empty() {
                continue;
            }

            let request = format!(
                r#"{{"jsonrpc": "2.0","method": "addPeers", "params": {:?}, "id": "1"}}"#,
                params
            );
            let response = send_request(&mut stream, &request)?;
            let response: bool = parse_response(&response)?;
            if response {
                println!("Successfully added peer addresses: {:?}", params);
            } else {
                bail!("Failed to add peer addresses: {:?}", params);
            }
        }
    } else {
        let params: Vec<String> = peers.into_iter().map(|addr| addr.to_string()).collect();
        let request = format!(
            r#"{{"jsonrpc": "2.0","method": "addPeers", "params": {:?}, "id": "1"}}"#,
            params
        );
        let response = send_request(&mut stream, &request)?;
        let response: bool = parse_response(&response)?;
        if response {
            println!("Successfully added peer addresses: {:?}", params);
        } else {
            bail!("Failed to add peer addresses: {:?}", params);
        }
    }

    Ok(())
}

pub fn clear_peers(addr: SocketAddr) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;

    let request = r#"{"jsonrpc": "2.0","method": "clearPeers", "id": "1"}"#;

    let response = send_request(&mut stream, request)?;
    let response: bool = parse_response(&response)?;
    if response {
        println!("Successfully cleared peers from buckets");
    } else {
        bail!("Failed to clear peers");
    }

    Ok(())
}

pub fn initialize_peers(addr: SocketAddr) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;

    let request = r#"{"jsonrpc": "2.0","method": "initializePeers", "id": "1"}"#;

    let response = send_request(&mut stream, request)?;
    let response: bool = parse_response(&response)?;
    if response {
        println!("Successfully cleared peers from buckets and initialized to config");
    } else {
        bail!("Failed to clear and initializepeers");
    }

    Ok(())
}

pub fn rewind(addr: SocketAddr, epoch: Epoch) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;

    let params = (epoch,);
    let request = format!(
        r#"{{"jsonrpc": "2.0","method": "rewind", "params": {}, "id": "1"}}"#,
        serde_json::to_string(&params)?
    );

    let response = send_request(&mut stream, &request)?;
    let response: bool = parse_response(&response)?;
    if response {
        println!("Started rewind process up to epoch {}.", params.0);
        println!("Use the nodeStats command to check the progress.");
    } else {
        bail!("Failed to rewind chain");
    }

    Ok(())
}

pub fn signaling_info(addr: SocketAddr) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;

    let request = r#"{"jsonrpc": "2.0","method": "signalingInfo", "id": "1"}"#;

    let response = send_request(&mut stream, request)?;
    let signaling_info: SignalingInfo = parse_response(&response)?;

    println!("Current epoch: {}", signaling_info.epoch);
    println!("\nList of activated upgrades:");
    let sorted_upgrades = signaling_info
        .active_upgrades
        .iter()
        .sorted_by(|a, b| a.1.cmp(b.1));
    for (upgrade, epoch) in sorted_upgrades {
        println!("- Epoch {}: {}", epoch, upgrade);
    }
    println!("\nList of pending upgrades:");
    for i in signaling_info.pending_upgrades {
        if i.init < signaling_info.epoch {
            let mut next_check = i.init + i.period;
            while next_check < signaling_info.epoch {
                next_check += i.period;
            }
            println!(
                "- {} (using bit {}): Started in {}. Next check will be on {}",
                i.wip, i.bit, i.init, next_check
            );

            let blocks_last_period = signaling_info
                .epoch
                .saturating_sub(next_check.saturating_sub(i.period));
            let signaling_blocks = i.votes;
            let non_signaling_block = blocks_last_period.saturating_sub(i.votes);
            let upcoming_blocks = next_check.saturating_sub(signaling_info.epoch);
            println!(
                "    Blocks: {} signaling, {} non-signaling, {} upcoming",
                signaling_blocks, non_signaling_block, upcoming_blocks
            );

            let percentage = i.votes.saturating_mul(100) / i.period;
            let relative_percentage =
                i.votes.saturating_mul(100) / std::cmp::max(1, blocks_last_period);
            println!(
                "    Total percentage achieved: {}%. Relative percentage: {}%",
                percentage, relative_percentage
            );

            let percentage_target = 80;
            let max_possible_votes = i.votes + upcoming_blocks;
            let max_possible_percentage = max_possible_votes.saturating_mul(100) / i.period;
            if percentage >= percentage_target {
                println!("    Will be activated in this period");
            } else if max_possible_percentage < percentage_target {
                println!("    Will not be activated in this period");
            } else if relative_percentage >= percentage_target {
                println!("    Will probably be activated in this period");
            } else {
                println!("    Will probably not be activated in this period");
            }
        } else {
            println!("- {} (using bit {}): Starts in {}", i.wip, i.bit, i.init);
        }
    }

    Ok(())
}

pub fn priority(addr: SocketAddr, json: bool) -> Result<(), failure::Error> {
    // Perform the JSONRPC request to the indicated node
    let mut stream = start_client(addr)?;
    let (estimate, (_, response)): (PrioritiesEstimate, _) =
        issue_method("priority", None::<serde_json::Value>, &mut stream, None)?;

    // JSON mode skips parsing of the JSONRPC response and rather outputs the JSON string as such
    if json {
        println!("{}", response);
    } else {
        println!("{}", estimate);
    }

    Ok(())
}

pub fn query_stakes(
    addr: SocketAddr,
    validator: Option<String>,
    withdrawer: Option<String>,
    long: bool,
) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let params = match (validator, withdrawer) {
        (Some(validator), Some(withdrawer)) => QueryStakes {
            filter: QueryStakesFilter::Key((
                MagicEither::Left(validator),
                MagicEither::Left(withdrawer),
            )),
            limits: QueryStakesLimits::default(),
        },
        (Some(validator), _) => QueryStakes {
            filter: QueryStakesFilter::Validator(MagicEither::Left(validator)),
            limits: QueryStakesLimits::default(),
        },
        (_, Some(withdrawer)) => QueryStakes {
            filter: QueryStakesFilter::Withdrawer(MagicEither::Left(withdrawer)),
            limits: QueryStakesLimits::default(),
        },
        (None, None) => QueryStakes::default(),
    };

    let response = send_request(
        &mut stream,
        &format!(
            r#"{{"jsonrpc": "2.0","method": "queryStakes", "params": {}, "id": 1}}"#,
            serde_json::to_string(&params).unwrap()
        ),
    )?;

    let mut stakes: Vec<StakeEntry<WIT_DECIMAL_PLACES, PublicKeyHash, Wit, Epoch, u64, u64>> =
        parse_response(&response)?;
    stakes.sort_by_key(|stake| Reverse(stake.value.coins));

    let mut stakes_table = Table::new();
    stakes_table.set_format(*prettytable::format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    if long {
        stakes_table.set_titles(row![c->"Validator", c->"Withdrawer", c->"Staked", c->"Mining Epoch", c->"Witnessing Epoch", c->"Nonce"]);
        for stake in stakes {
            stakes_table.add_row(row![
                stake.key.validator,
                stake.key.withdrawer,
                r->whole_wits(stake.value.coins.into()).to_formatted_string(&Locale::en),
                r->stake.value.epochs.mining.to_formatted_string(&Locale::en),
                r->stake.value.epochs.witnessing.to_formatted_string(&Locale::en),
                r->stake.value.nonce.to_formatted_string(&Locale::en),
            ]);
        }
    } else {
        stakes_table.set_titles(row![c->"Validator", c->"Withdrawer", c->"Staked"]);
        for stake in stakes {
            stakes_table.add_row(row![
                stake.key.validator,
                stake.key.withdrawer,
                r->whole_wits(stake.value.coins.into()).to_formatted_string(&Locale::en),
            ]);
        }
    }
    stakes_table.printstd();
    println!();

    Ok(())
}

pub fn query_powers(
    addr: SocketAddr,
    capability: Option<String>,
    all: bool,
) -> Result<(), failure::Error> {
    let mut stream = start_client(addr)?;
    let params = if all {
        QueryPowersParams::All(true)
    } else {
        match capability {
            Some(c) => match Capability::from_str(&c) {
                Ok(c) => QueryPowersParams::Capability(c),
                Err(_) => QueryPowersParams::Capability(Capability::Mining),
            },
            None => QueryPowersParams::Capability(Capability::Mining),
        }
    };

    let response = send_request(
        &mut stream,
        &format!(
            r#"{{"jsonrpc": "2.0","method": "queryPowers", "params": {}, "id": 1}}"#,
            serde_json::to_string(&params).unwrap()
        ),
    )?;

    let mut powers: Vec<QueryPowersRecord> = parse_response(&response)?;
    powers.sort_by_key(|power| Reverse(power.powers[0]));

    let mut powers_table = Table::new();
    powers_table.set_format(*prettytable::format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    if all {
        let mut header = vec![
            Cell::new("Validator").style_spec("c"),
            Cell::new("Withdrawer").style_spec("c"),
        ];
        for capability in ALL_CAPABILITIES {
            let capability_str: &'static str = capability.into();
            header.push(Cell::new(capability_str).style_spec("c"));
        }
        powers_table.set_titles(Row::new(header));
        for power in powers.iter() {
            let mut row = vec![
                Cell::new(&power.validator.to_string()),
                Cell::new(&power.withdrawer.to_string()),
            ];
            for p in &power.powers {
                row.push(Cell::new(&p.to_formatted_string(&Locale::en)).style_spec("r"));
            }
            powers_table.add_row(Row::new(row));
        }
    } else {
        let capability_str: &'static str = params.into();
        powers_table.set_titles(row![c->"Validator", c->"Withdrawer", c->capability_str]);
        for power in powers.iter() {
            powers_table.add_row(row![
                power.validator,
                power.withdrawer,
                r->(power.powers[0]).to_formatted_string(&Locale::en),
            ]);
        }
    }
    powers_table.printstd();
    println!();

    Ok(())
}

#[derive(Serialize, Deserialize)]
struct SignatureWithData {
    address: String,
    identifier: String,
    public_key: String,
    signature: String,
}

pub fn claim(
    addr: SocketAddr,
    identifier: String,
    write_to_path: Option<&Path>,
) -> Result<(), failure::Error> {
    if identifier.is_empty() || identifier.trim() != identifier {
        bail!("Claiming identifier cannot be empty or start/end with empty spaces");
    }

    let request = format!(
        r#"{{"jsonrpc": "2.0","method": "sign", "params": {:?}, "id": "1"}}"#,
        calculate_sha256(identifier.as_bytes()).as_ref(),
    );

    let mut stream = start_client(addr)?;
    let response = send_request(&mut stream, &request)?;

    let signature: KeyedSignature = parse_response(&response)?;
    match serde_json::to_string_pretty(&SignatureWithData {
        identifier: identifier.clone(),
        address: PublicKeyHash::from_public_key(&signature.public_key).to_string(),
        public_key: signature
            .public_key
            .to_bytes()
            .iter()
            .fold(String::new(), |acc, x| format!("{}{:02x}", acc, x)),
        signature: signature
            .signature
            .to_bytes()?
            .iter()
            .fold(String::new(), |acc, x| format!("{}{:02x}", acc, x)),
    }) {
        Ok(signed_data) => {
            if let Some(base_path) = write_to_path {
                let path = base_path.join(format!(
                    "claim-{}-{}.txt",
                    identifier,
                    PublicKeyHash::from_public_key(&signature.public_key)
                ));
                let mut file = File::create(&path)?;
                file.write_all(format!("{}\n", signed_data).as_bytes())?;
                let full_path = Path::new(&path);
                println!(
                    "Signed claiming data written to {}",
                    full_path.canonicalize()?.as_path().display()
                );
            } else {
                println!("Signed claiming data:\n{}", signed_data);
            }
        }
        Err(error) => bail!("Failed to sign claiming data: {:?}", error),
    }

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
    result: T,
}

/// A failed request returns an error with code and message
#[derive(Debug, Deserialize)]
struct JsonRpcError {
    error: ServerError,
}

/// Id. Can be null, a number, or a string
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Id {
    Null,
    Number(),
    String(),
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
    log::trace!("> {}", request);
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
    let mut response = String::new();
    r.read_line(&mut response)?;
    log::trace!("< {}", response);
    Ok(response)
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
            let error_json: JsonRpcError = serde_json::from_str(response)?;
            Err(error_json.error.into())
        }
    }
}

/// Unwraps an `Option<Fee>` representing a fee, returning also a priority estimate if it was `None`.
fn unwrap_fee_or_estimate_priority<S>(
    fee: Option<Fee>,
    stream: &mut S,
    id: &mut SequentialId<u8>,
) -> Result<(Fee, Option<PrioritiesEstimate>), failure::Error>
where
    S: Read + Write,
{
    Ok(match fee {
        None => {
            let (estimate, _) =
                issue_method("priority", None::<serde_json::Value>, stream, id.next())?;

            (Fee::default(), Some(estimate))
        }
        Some(fee) => (fee, None),
    })
}

/// Perform a JSON-RPC query directed to a specified Witnet node, and return the output as a
/// specified type, along the request and the response strings.
fn issue_method<M, S, P, O>(
    method: M,
    params: Option<P>,
    stream: &mut S,
    id: Option<u8>,
) -> Result<(O, (String, String)), failure::Error>
where
    M: Into<String>,
    S: Read + Write,
    P: Default + Serialize,
    O: DeserializeOwned,
{
    let request = format!(
        r#"{{"jsonrpc": "2.0","method": "{}", "params": {}, "id": "{}"}}"#,
        method.into(),
        serde_json::to_string(&params.unwrap_or_default())?,
        id.unwrap_or(1)
    );
    let response = send_request(stream, &request)?;
    parse_response::<O>(&response).map(|output| (output, (request, response)))
}

fn prompt_user_for_priority_selection(
    estimates: Vec<(&str, Priority, Fee, TimeToBlock)>,
) -> Result<Fee, failure::Error> {
    // Time to print the estimates
    println!("[ Fee suggestions ]");
    println!("Please choose one of the following options depending on how urgently you want this transaction to be mined into a block:");
    for (i, (label, priority, fee, time_to_block, ..)) in estimates.iter().enumerate() {
        println!(
            "[{}] {} (around {}) → {} Wit (priority = {})",
            i, label, time_to_block, fee, priority
        );
    }
    let options = (0..estimates.len()).map(|x| usize::to_string(&x)).join("/");

    // This is where we prompt the user for typing the desired priority tier from the options
    // printed above. This is done in a loop until a valid option is selected.
    let mut input = String::new();
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let fee;
    loop {
        print!(
            "Please type the number of your preferred priority tier and press Enter ({}): ",
            options
        );
        io::stdout().flush()?;
        input.clear();
        stdin.read_line(&mut input)?;

        let selected: usize = input.trim().parse()?;
        if let Some((label, priority, selected_fee, time_to_block)) =
            estimates.get(selected).cloned()
        {
            fee = selected_fee;
            println!(
                "You have selected priority tier ({}) \"{}\"\n- A fee of {} Wit will be used\n- Priority is {}.\n- The expected time-to-block is {}",
                selected, label, selected_fee, priority, time_to_block
            );
            break;
        } else {
            eprintln!(r#""{}" is not a valid option."#, input.trim());
        }
    }

    Ok(fee)
}

fn prompt_user_for_stake_confirmation(data: &BuildStakeResponse) -> Result<bool, failure::Error> {
    let environment = get_environment();
    let value = Wit::from_nanowits(data.transaction.body.output.value).to_string();

    // Time to print the data
    println!("╔══════════════════════════════════════════════════════════════════════════════╗");
    println!("║                  PLEASE CAREFULLY REVIEW THE DATA BELOW                      ║");
    println!("╟──────────────────────────────────────────────────────────────────────────────╢");
    println!("║ Failing to review this information diligently may result in stakes that      ║");
    println!("║ cannot be operated or withdrawn, i.e. loss of funds.                         ║");
    println!("╠══════════════════════════════════════════════════════════════════════════════╣");
    println!("║ 1. STAKER ADDRESSES                                                          ║");
    println!("║    These are the addresses from which the coins to stake will be sourced.    ║");
    println!("║    None of these addresses will be able to unstake or spend the staked       ║");
    println!("║    coins, unless one of them is also the withdrawer address below.           ║");
    println!("║                                                                              ║");
    for (i, address) in data
        .staker
        .iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .enumerate()
    {
        let address = address.bech32(environment);
        println!("║    #{:0>2}: {: <69}║", i, address);
    }
    println!("╟──────────────────────────────────────────────────────────────────────────────╢");
    println!("║ 2. VALIDATOR ADDRESS                                                         ║");
    println!("║    This is the address of the node that will be operating the staked coins.  ║");
    println!("║    The validator will not be able to unstake or spend the staked coins —     ║");
    println!("║    that role is reserved for the withdrawer address below.                   ║");
    println!("║                                                                              ║");
    println!(
        "║    Validator address: {: <55}║",
        data.validator.bech32(environment)
    );
    println!("╟──────────────────────────────────────────────────────────────────────────────╢");
    println!("║ 3. WITHDRAWER ADDRESS                                                        ║");
    println!("║    This is the only address that will be allowed to unstake and eventually   ║");
    println!("║    spend the staked coins, and the accumulated rewards if any.               ║");
    println!("║    This MUST belong to your wallet, otherwise you may be giving away or      ║");
    println!("║    or burning your coins.                                                    ║");
    println!("║                                                                              ║");
    println!(
        "║    Withdrawer address: {: <54}║",
        data.withdrawer.bech32(environment)
    );
    println!("╟──────────────────────────────────────────────────────────────────────────────╢");
    println!("║ 4. STAKE AMOUNT                                                              ║");
    println!("║    This is the number of coins that will be staked. While staked, the coins  ║");
    println!("║    cannot be transferred or spent. They can only be unstaked and eventually  ║");
    println!("║    spent by the withdrawer address above.                                    ║");
    println!("║                                                                              ║");
    println!("║    Stake amount: {} {: <42}║", value, "Wit coins");
    println!("╚══════════════════════════════════════════════════════════════════════════════╝");

    // This is where we prompt the user for typing the desired priority tier from the options
    // printed above. This is done in a loop until a valid option is selected.
    let mut input = String::new();
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    loop {
        print!("Please double-check the information above and confirm if it is correct (y/N): ",);
        io::stdout().flush()?;
        input.clear();
        stdin.read_line(&mut input)?;
        let selected = input.trim().to_uppercase();

        if ["Y", "YES"].contains(&selected.as_str()) {
            return Ok(true);
        } else if ["", "N", "NO"].contains(&selected.as_str()) {
            break;
        }
    }

    Ok(false)
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
        let block_chain: Result<ResponseBlockChain<'_>, _> = parse_response(response);
        assert!(block_chain.is_err());
    }

    #[test]
    fn parse_get_block_chain() {
        let response = r#"{"jsonrpc":"2.0","result":[[0,"ed28899af8c3148a4162736af942bc68c4466da93c5124dabfaa7c582af49e30"],[1,"9c9038cfb31a7050796920f91b17f4a68c7e9a795ee8962916b35d39fc1efefc"]],"id":1}"#;
        let block_chain: ResponseBlockChain<'_> = parse_response(response).unwrap();
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

    #[test]
    fn verify_claim_output() {
        use witnet_crypto::signature::{
            verify, PublicKey as SecpPublicKey, Signature as SecpSignature,
        };

        let json_output = r#"
        {
          "address": "twit17k4tzsf9zs70q8ndur7qvavvhvrkfd8jkjrppw",
          "identifier": "WITNET_000",
          "public_key": "038f48d48aaa177c54809598649a037fb75a391449c8d0fee3f7d3b7f8fcd48239",
          "signature": "a1a37548b1367dd683b87abf534aafa5c9c3c9c15fd4186d437180a61e7bd31e585cf36ff2fddbc6ad5bbdddb65c2195895f855b60a7b81f44a100288a821561"
        }"#;

        // Parse the string of data into serde_json::Value.
        let signature_with_data: SignatureWithData = serde_json::from_str(json_output).unwrap();

        // Check address is correctly derived from public key
        let address = PublicKeyHash::from_public_key(
            &PublicKey::try_from_slice(
                &hex::decode(signature_with_data.public_key.clone()).unwrap(),
            )
            .unwrap(),
        )
        .bech32(Environment::Testnet);
        assert_eq!(address, signature_with_data.address);

        // Required fields for Secpk1 signature verification
        let signed_data = calculate_sha256(signature_with_data.identifier.as_bytes());
        let public_key =
            SecpPublicKey::from_slice(&hex::decode(signature_with_data.public_key).unwrap())
                .unwrap();
        let signature =
            SecpSignature::from_compact(&hex::decode(signature_with_data.signature).unwrap())
                .unwrap();

        assert!(verify(&public_key, signed_data.as_ref(), &signature).is_ok());
    }
}
