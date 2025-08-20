use crate::{
    actors::dr_database::{DrDatabase, DrId, DrState, SetDrState},
    config::Config,
    handle_receipt,
};
use actix::prelude::*;
use std::{collections::HashSet, sync::Arc, time::Duration};
use web3::{
    Web3,
    contract::{self, Contract, tokens::Tokenize},
    ethabi::{Token, ethereum_types::H256},
    transports::Http,
    types::{H160, U256},
};
use web3_unit_converter::Unit;
use witnet_data_structures::{chain::Hash, radon_error::RadonErrors};
use witnet_node::utils::stop_system_if_panicking;

/// DrReporter actor sends the the Witnet Request tally results to Ethereum
#[derive(Default)]
pub struct DrReporter {
    /// Web3
    pub web3: Option<Web3<Http>>,
    /// WRB contract
    pub wrb_contract: Option<Arc<Contract<web3::transports::Http>>>,
    /// EVM account used to report data request results
    pub eth_from: H160,
    /// EVM account minimum balance under which alerts will be logged
    pub eth_from_balance_threshold: u64,
    /// Flag indicating whether low funds alert was already logged
    pub eth_from_balance_alert: bool,
    /// report_result_limit
    pub eth_max_gas: Option<u64>,
    /// Price of $nanoWit in Wei, used to improve estimation of report profits
    pub eth_nanowit_wei_price: Option<u64>,
    /// Max time to wait for an ethereum transaction to be confirmed before returning an error
    pub eth_txs_timeout_ms: u64,
    /// Number of block confirmations needed to assume finality when sending transactions to ethereum
    pub eth_txs_confirmations: usize,
    /// maximum result size (in bytes)
    pub witnet_dr_max_result_size: usize,
    /// Pending reportResult transactions. The actor should not attempt to report these requests
    /// until the timeout has elapsed
    pub pending_dr_reports: HashSet<DrId>,
}

impl Drop for DrReporter {
    fn drop(&mut self) {
        log::trace!("Dropping DrReporter");
        stop_system_if_panicking("DrReporter");
    }
}

/// Make actor from EthPoller
impl Actor for DrReporter {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("DrReporter actor has been started!");
    }
}

/// Required trait for being able to retrieve DrReporter address from system registry
impl actix::Supervised for DrReporter {}

/// Required trait for being able to retrieve DrReporter address from system registry
impl SystemService for DrReporter {}

impl DrReporter {
    /// Initialize `DrReporter` taking the configuration from a `Config` structure
    pub fn from_config(
        config: &Config,
        web3: Web3<Http>,
        wrb_contract: Arc<Contract<Http>>,
    ) -> Self {
        Self {
            web3: Some(web3),
            wrb_contract: Some(wrb_contract),
            eth_from: config.eth_from,
            eth_from_balance_threshold: config.eth_from_balance_threshold,
            eth_from_balance_alert: false,
            eth_max_gas: config.eth_gas_limits.report_result,
            eth_nanowit_wei_price: config.eth_nanowit_wei_price,
            eth_txs_timeout_ms: config.eth_txs_timeout_ms,
            eth_txs_confirmations: config.eth_txs_confirmations,
            witnet_dr_max_result_size: config.witnet_dr_max_result_size,
            pending_dr_reports: Default::default(),
        }
    }
}

/// Report the results of these data requests to Ethereum
pub struct DrReporterMsg {
    /// Reports
    pub reports: Vec<Report>,
}

/// Report the result of this data request id to ethereum
pub struct Report {
    /// Data Request's unique query id as known by the WitnetOracle contract
    pub dr_id: DrId,
    /// Timestamp at which the reported result was actually generated
    pub dr_timestamp: i64,
    /// Hash of the Data Request Transaction in the Witnet blockchain
    pub dr_tx_hash: Hash,
    /// Hash of the Data Request Tally Transaction in the Witnet blockchain
    pub dr_tally_tx_hash: Hash,
    /// CBOR-encoded result to Data Request, as resolved by the Witnet blockchain
    pub result: Vec<u8>,
}

impl Message for DrReporterMsg {
    type Result = ();
}

impl Handler<DrReporterMsg> for DrReporter {
    type Result = ();

    fn handle(&mut self, mut msg: DrReporterMsg, ctx: &mut Self::Context) -> Self::Result {
        // Remove all reports that have already been reported, but whose reporting transaction is still pending
        msg.reports.retain(|report| {
            if self.pending_dr_reports.contains(&report.dr_id) {
                // Timeout is not over yet, no action is needed
                log::debug!(
                    "[{}] => ignored as it's currently being reported",
                    report.dr_id
                );

                false
            } else {
                true
            }
        });

        if msg.reports.is_empty() {
            // Nothing to report
            return;
        }

        let dr_ids: Vec<_> = msg.reports.iter().map(|report| report.dr_id).collect();
        let incoming_dr_ids = dr_ids.clone();
        let wrb_contract = self.wrb_contract.clone().unwrap();
        let eth_from = self.eth_from;
        let eth_from_balance_threshold = self.eth_from_balance_threshold;
        let mut eth_from_balance_alert = self.eth_from_balance_alert;
        let eth_max_gas = self.eth_max_gas;
        let eth_txs_confirmations = self.eth_txs_confirmations;
        let eth_tx_timeout = Duration::from_millis(self.eth_txs_timeout_ms);
        let eth_nanowit_wei_price = U256::from(self.eth_nanowit_wei_price.unwrap_or_default());

        for report in &mut msg.reports {
            if report.result.len() > self.witnet_dr_max_result_size {
                let radon_error = RadonErrors::BridgeOversizedResult as u8;
                report.result = vec![0xD8, 0x27, 0x81, 0x18, radon_error]
            }
        }

        // New request or timeout elapsed, save dr_id
        for report in &msg.reports {
            self.pending_dr_reports.insert(report.dr_id);
        }

        let eth = self.web3.as_ref().unwrap().eth();
        let fut = async move {
            // Trace low funds alerts if required.
            let eth_from_balance = match eth.balance(eth_from, None).await {
                Ok(x) => {
                    if x < U256::from(eth_from_balance_threshold) {
                        eth_from_balance_alert = true;
                        log::warn!(
                            "EVM address {} running low of funds: {} ETH",
                            eth_from,
                            Unit::Wei(&x.to_string()).to_eth_str().unwrap_or_default()
                        );
                    } else if eth_from_balance_alert {
                        log::info!("EVM address {eth_from} recovered funds.");
                        eth_from_balance_alert = false;
                    }

                    x
                }
                Err(e) => {
                    log::error!("Error geting balance from address {eth_from}: {e:?}");

                    return eth_from_balance_alert;
                }
            };

            if msg.reports.is_empty() {
                // Nothing to report
                return eth_from_balance_alert;
            }

            // We don't want to proceed with reporting if there's no way to fetch the gas price
            // from the provider or gateway.
            let eth_gas_price = match eth.gas_price().await {
                Ok(x) => x,
                Err(e) => {
                    log::error!("Error estimating network gas price: {e}");

                    return eth_from_balance_alert;
                }
            };

            let batched_report: Vec<_> = msg
                .reports
                .iter()
                .map(|report| {
                    let dr_hash = H256::from_slice(report.dr_tx_hash.as_ref());
                    // the trait `web3::contract::tokens::Tokenize` is not implemented for
                    // `(std::vec::Vec<(web3::types::U256, web3::types::U256, web3::types::H256, std::vec::Vec<u8>)>, bool)
                    // Need to manually convert to tuple
                    Token::Tuple(vec![
                        Token::Uint(report.dr_id),
                        Token::Uint(report.dr_timestamp.into()),
                        Token::FixedBytes(dr_hash.to_fixed_bytes().to_vec()),
                        Token::Bytes(report.result.clone()),
                    ])
                })
                .collect();

            let batched_reports = split_by_gas_limit(
                batched_report,
                &wrb_contract,
                eth_from,
                eth_gas_price,
                eth_nanowit_wei_price,
                eth_max_gas,
            )
            .await;

            log::info!(
                "{:?} will be reported in {} transactions",
                dr_ids,
                batched_reports.len(),
            );

            for (batched_report, eth_gas_limit) in batched_reports {
                // log::debug!("Executing reportResultBatch {:?}", batched_report);

                let receipt_fut = wrb_contract.call_with_confirmations(
                    "reportResultBatch",
                    batched_report.clone(),
                    eth_from,
                    contract::Options::with(|opt| {
                        opt.gas = Some(eth_gas_limit);
                        opt.gas_price = Some(eth_gas_price);
                    }),
                    eth_txs_confirmations,
                );

                let receipt = tokio::time::timeout(eth_tx_timeout, receipt_fut).await;
                match receipt {
                    Ok(Ok(receipt)) => {
                        log::debug!("{dr_ids:?} <> {receipt:?}");
                        match handle_receipt(&receipt).await {
                            Ok(()) => {
                                let mut dismissed_dr_reports: HashSet<DrId> = Default::default();
                                for log in receipt.logs {
                                    if let Some((dismissed_dr_id, reason)) =
                                        parse_batch_report_error_log(wrb_contract.abi(), log)
                                        && dismissed_dr_reports.insert(dismissed_dr_id)
                                    {
                                        log::warn!(
                                            "[{dismissed_dr_id}] >< dismissed due to \"{reason}\" ..."
                                        );
                                    }
                                }
                                let dr_database_addr = DrDatabase::from_registry();
                                for report in &msg.reports {
                                    if dismissed_dr_reports.contains(&report.dr_id) {
                                        // Dismiss data requests that could not (or need not) get reported
                                        dr_database_addr
                                            .send(SetDrState {
                                                dr_id: report.dr_id,
                                                dr_state: DrState::Dismissed,
                                            })
                                            .await
                                            .ok();
                                    } else {
                                        // Finalize data requests that got successfully reported
                                        log::info!(
                                            "[{}] <= dr_tally_tx = {}",
                                            report.dr_id,
                                            report.dr_tally_tx_hash
                                        );
                                        dr_database_addr
                                            .send(SetDrState {
                                                dr_id: report.dr_id,
                                                dr_state: DrState::Finished,
                                            })
                                            .await
                                            .ok();
                                    }
                                }
                            }
                            Err(()) => {
                                log::error!(
                                    "reportResultBatch(..) tx reverted: {}",
                                    receipt.transaction_hash
                                );
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        // Error in call_with_confirmations
                        log::error!(
                            "{}: {:?}",
                            format_args!("Cannot call reportResultBatch{:?}", &batched_report),
                            e
                        );
                    }
                    Err(elapsed) => {
                        // Timeout is over
                        log::warn!(
                            "Timeout ({} secs) when calling reportResultBatch{:?}",
                            elapsed,
                            &batched_report
                        );
                    }
                }
            }

            if let Ok(x) = eth.balance(eth_from, None).await {
                match x.cmp(&eth_from_balance) {
                    std::cmp::Ordering::Less => {
                        log::warn!(
                            "EVM address {} loss = -{} ETH",
                            eth_from,
                            Unit::Wei(&(eth_from_balance - x).to_string())
                                .to_eth_str()
                                .unwrap_or_default()
                        );
                    }
                    std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => {
                        log::debug!(
                            "EVM address {} revenue = +{} ETH",
                            eth_from,
                            Unit::Wei(&(x - eth_from_balance).to_string())
                                .to_eth_str()
                                .unwrap_or_default()
                        );
                        eth_from_balance_alert = false;
                    }
                }
            }

            eth_from_balance_alert
        };

        ctx.spawn(fut.into_actor(self).map(
            move |eth_from_balance_alert, act, _ctx: &mut Context<DrReporter>| {
                // Reset timeouts
                for dr_id in incoming_dr_ids {
                    act.pending_dr_reports.remove(&dr_id);
                }
                act.eth_from_balance_alert = eth_from_balance_alert
            },
        ));
    }
}

/// Get the queryId of a PostedResult event, or return None if this is a different kind of event
fn parse_batch_report_error_log(
    wrb_contract_abi: &web3::ethabi::Contract,
    log: web3::types::Log,
) -> Option<(DrId, String)> {
    let batch_report_error = wrb_contract_abi.events_by_name("BatchReportError").unwrap();
    // There should be exactly one PostedResult event declartion within the ABI
    assert_eq!(batch_report_error.len(), 1);
    let batch_report_error = &batch_report_error[0];
    // Parse log, ignoring it if the topic does not match "BatchReportError"
    let batch_report_error_log = batch_report_error
        .parse_log(web3::ethabi::RawLog {
            topics: log.topics,
            data: log.data.0,
        })
        .ok()?;
    let batch_report_error_log_params = batch_report_error_log.params;
    let query_id = &batch_report_error_log_params[0];
    assert_eq!(query_id.name, "queryId");
    let reason = &batch_report_error_log_params[1];
    assert_eq!(reason.name, "reason");
    match (&query_id.value, &reason.value) {
        (Token::Uint(query_id), Token::String(reason)) => Some((*query_id, reason.to_string())),
        _ => {
            log::error!("Invalid BatchReportError params: {batch_report_error_log_params:?}");
            None
        }
    }
}

/// Split a batched report (argument of reportResultBatch) into multiple smaller
/// batched reports in order to fit into some gas limit.
///
/// Returns a list of `(batched_report, estimated_gas)` that should be used to
/// create multiple "reportResultBatch" transactions.
async fn split_by_gas_limit(
    batched_report: Vec<Token>,
    wrb_contract: &Contract<Http>,
    eth_from: H160,
    eth_gas_price: U256,
    eth_nanowit_wei_price: U256,
    eth_max_gas: Option<u64>,
) -> Vec<(Vec<Token>, U256)> {
    let mut v = vec![];
    let mut stack = vec![batched_report];

    while let Some(batch_params) = stack.pop() {
        let eth_report_result_batch_params = batch_params.clone();

        // --------------------------------------------------------------------------
        // First: try to estimate gas required for reporting this batch of tuples ...

        let estimated_gas = wrb_contract
            .estimate_gas(
                "reportResultBatch",
                eth_report_result_batch_params.clone(),
                eth_from,
                contract::Options::with(|opt| {
                    opt.gas = eth_max_gas.map(Into::into);
                    opt.gas_price = Some(eth_gas_price);
                }),
            )
            .await;

        if let Err(e) = estimated_gas {
            if batch_params.len() <= 1 {
                // Skip this single-query batch if still not possible to estimate gas
                log::error!("Cannot estimate gas limit: {e:?}");
                log::warn!("Skipping report batch: {batch_params:?}");
            } else {
                // Split batch in half if gas estimation is not possible
                let (batch_tuples_1, batch_tuples_2) =
                    batch_params.split_at(batch_params.len() / 2);
                stack.push(batch_tuples_1.to_vec());
                stack.push(batch_tuples_2.to_vec());
            }

            continue;
        }

        let estimated_gas = estimated_gas.unwrap();
        log::debug!(
            "reportResultBatch (x{} drs) estimated gas: {:?}",
            batch_params.len(),
            estimated_gas
        );

        // ------------------------------------------------
        // Second: try to estimate actual profit, if any...

        let query_ids: Vec<Token> = batch_params
            .iter()
            .map(|report_params| {
                if let Token::Tuple(report_params) = report_params {
                    assert_eq!(report_params.len(), 4);

                    report_params[0].clone()
                } else {
                    panic!("Cannot extract query id from batch tuple");
                }
            })
            .collect();

        // the size of the report result tx data may affect the actual profit
        // on some layer-2 EVM chains:
        let eth_report_result_batch_msg_data = wrb_contract
            .abi()
            .function("reportResultBatch")
            .and_then(|f| f.encode_input(&eth_report_result_batch_params.into_tokens()));

        let params = (
            Token::Array(query_ids),
            Token::Bytes(eth_report_result_batch_msg_data.unwrap_or_default()),
            Token::Uint(eth_gas_price),
            Token::Uint(eth_nanowit_wei_price),
        );

        let estimated_profit: Result<(U256, U256), web3::contract::Error> = wrb_contract
            .query(
                "estimateReportEarnings",
                params,
                eth_from,
                contract::Options::with(|opt| {
                    opt.gas = eth_max_gas.map(Into::into);
                    opt.gas_price = Some(eth_gas_price);
                }),
                None,
            )
            .await;

        match estimated_profit {
            Ok((revenues, expenses)) => {
                log::debug!(
                    "reportResultBatch (x{} drs) estimated profit: {} - {} ETH",
                    batch_params.len(),
                    Unit::Wei(&revenues.to_string())
                        .to_eth_str()
                        .unwrap_or_default(),
                    Unit::Wei(&expenses.to_string())
                        .to_eth_str()
                        .unwrap_or_default(),
                );
                v.push((batch_params, estimated_gas));
                continue;
            }
            // Ok(_) => {
            //     if batch_params.len() <= 1 {
            //         log::warn!("Skipping unprofitable report: {:?}", batch_params);
            //     }
            // }
            Err(e) => {
                if batch_params.len() <= 1 {
                    log::error!("Cannot estimate report profit: {e:?}");
                }
            }
        };

        if batch_params.len() > 1 {
            // Split batch in half if no profit, or no profit estimation was possible
            let (sub_batch_1, sub_batch_2) = batch_params.split_at(batch_params.len() / 2);
            stack.push(sub_batch_1.to_vec());
            stack.push(sub_batch_2.to_vec());
        }
    }

    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hack_fix_functions_with_multiple_definitions;
    use web3::contract::tokens::Tokenize;

    /// Returns `a / b`, as f64
    fn u256_div_as_f64(a: U256, b: U256) -> f64 {
        u256_to_f64(a) / u256_to_f64(b)
    }

    /// Converts `U256` into `f64` in a lossy way
    fn u256_to_f64(a: U256) -> f64 {
        a.to_string().parse().unwrap()
    }

    /// Returns `a * b` as U256, saturating on overflow
    fn u256_saturating_mul_f64(a: U256, b: f64) -> U256 {
        assert!(
            b >= 0.0,
            "u256_mul_f64 only supports positive floating point values, got {b}"
        );

        // Prevent doing any further calculations if we're multiplying zero by something else.
        if a == U256::zero() || b == 0.0 {
            return U256::zero();
        }

        // Binary search a value x such that x / a == b
        let mut lo = U256::from(0);
        let mut hi = U256::MAX;
        // mid = (lo + hi) / 2, but avoid overflows
        let mut mid = lo / 2 + hi / 2;

        loop {
            let ratio = u256_div_as_f64(mid, a);

            if ratio == b {
                break mid;
            }
            if ratio > b {
                hi = mid;
            }
            if ratio < b {
                lo = mid;
            }

            let new_mid = lo / 2 + hi / 2;
            if new_mid == mid {
                if ratio > b {
                    break lo;
                }
                if ratio < b {
                    break hi;
                }
            }
            mid = new_mid;
        }
    }

    fn unwrap_batch(t: Token) -> (Token, Token, Token, Token) {
        if let Token::Tuple(token_vec) = t {
            assert_eq!(token_vec.len(), 4);
            (
                token_vec[0].clone(),
                token_vec[1].clone(),
                token_vec[2].clone(),
                token_vec[3].clone(),
            )
        } else {
            panic!("Token:Tuple not found in unwrap_batch function");
        }
    }

    #[ignore]
    #[test]
    fn report_result_type_check() {
        let wrb_contract_abi_json: &[u8] = include_bytes!("../../../wrb_abi.json");
        let mut wrb_contract_abi = web3::ethabi::Contract::load(wrb_contract_abi_json)
            .map_err(|e| format!("Unable to load WRB contract from ABI: {e:?}"))
            .unwrap();
        hack_fix_functions_with_multiple_definitions(&mut wrb_contract_abi);

        let msg = DrReporterMsg {
            reports: vec![Report {
                dr_id: DrId::from(4358u32),
                dr_timestamp: 0,
                dr_tx_hash: Hash::SHA256([
                    106, 107, 78, 5, 218, 5, 159, 172, 215, 12, 141, 98, 19, 163, 167, 65, 62, 79,
                    3, 170, 169, 162, 186, 24, 59, 135, 45, 146, 133, 85, 250, 155,
                ]),
                dr_tally_tx_hash: Hash::SHA256([
                    106, 107, 78, 5, 218, 5, 159, 172, 215, 12, 141, 98, 19, 163, 167, 65, 62, 79,
                    3, 170, 169, 162, 186, 24, 59, 135, 45, 146, 133, 85, 250, 155,
                ]),
                result: vec![26, 160, 41, 182, 230],
            }],
        };

        let batch_results: Vec<_> = msg
            .reports
            .iter()
            .map(|report| {
                let dr_hash = H256::from_slice(report.dr_tx_hash.as_ref());

                // the trait `web3::contract::tokens::Tokenize` is not implemented for
                // `(std::vec::Vec<(web3::types::U256, web3::types::U256, web3::types::H256, std::vec::Vec<u8>)>, bool)
                // Need to manually call `.into_tokens()`:
                Token::Tuple(vec![
                    Token::Uint(report.dr_id),
                    Token::Uint(report.dr_timestamp.into()),
                    Token::FixedBytes(dr_hash.to_fixed_bytes().to_vec()),
                    Token::Bytes(report.result.clone()),
                ])
            })
            .collect();

        let params_one = unwrap_batch(batch_results[0].clone());
        wrb_contract_abi
            .function("reportResult")
            .and_then(|function| function.encode_input(&params_one.into_tokens()))
            .expect("encode args failed");

        let params_batch = batch_results;
        wrb_contract_abi
            .function("reportResultBatch")
            .and_then(|function| function.encode_input(&params_batch.into_tokens()))
            .expect("encode args failed");
    }

    #[ignore]
    #[test]
    fn parse_logs_report_result_batch() {
        let wrb_contract_abi_json: &[u8] = include_bytes!("../../../wrb_abi.json");
        let mut wrb_contract_abi = web3::ethabi::Contract::load(wrb_contract_abi_json)
            .map_err(|e| format!("Unable to load WRB contract from ABI: {e:?}"))
            .unwrap();
        hack_fix_functions_with_multiple_definitions(&mut wrb_contract_abi);

        let log_posted_result = web3::types::Log {
            address: "0x8ab653b73a0e0552dddce8c76f97c6aa826efbd4"
                .parse()
                .unwrap(),
            topics: vec![
                "0x4df64445edc775fba59db44b8001852fb1b777eea88fd54f04572dd114e3ff7f"
                    .parse()
                    .unwrap(),
            ],
            data: web3::types::Bytes(hex::decode("0000000000000000000000000000000000000000000000000000000000001b58000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000146e6f7420696e20506f7374656420737461747573000000000000000000000000").unwrap()),
            block_hash: None,
            block_number: None,
            transaction_hash: None,
            transaction_index: None,
            log_index: None,
            transaction_log_index: None,
            log_type: None,
            removed: None,
        };
        assert_eq!(
            parse_batch_report_error_log(&wrb_contract_abi, log_posted_result),
            Some((U256::from(7_000), String::from("not in Posted status"),))
        );
    }

    #[test]
    fn test_u256_mul_f64() {
        let x = u256_saturating_mul_f64(U256::from(1_000_000), 0.0);
        assert_eq!(x, U256::from(0));
        let x = u256_saturating_mul_f64(U256::from(1_000_000), 0.5);
        assert_eq!(x, U256::from(500_000));
        let x = u256_saturating_mul_f64(U256::from(1_000_000), 1.0);
        assert_eq!(x, U256::from(1_000_000));
        let x = u256_saturating_mul_f64(U256::from(1_000_000), 1.3);
        assert_eq!(x, U256::from(1_300_000));
        let x = u256_saturating_mul_f64(U256::from(1_000_000), 1.5);
        assert_eq!(x, U256::from(1_500_000));
        let x = u256_saturating_mul_f64(U256::from(1_000_000), f64::INFINITY);
        assert_eq!(x, U256::MAX);
    }
}
