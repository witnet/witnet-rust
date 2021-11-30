use crate::{
    actors::{
        dr_database::{
            DrDatabase, DrId, DrInfoBridge, DrState, SetDrInfoBridge, WitnetQueryStatus,
        },
        eth_poller::process_reported_request,
    },
    config::Config,
    handle_receipt,
};
use actix::prelude::*;
use std::{collections::HashSet, sync::Arc, time::Duration};
use web3::{
    contract::{self, Contract},
    ethabi::{ethereum_types::H256, Bytes, Token},
    transports::Http,
    types::{H160, U256},
};
use witnet_data_structures::{chain::Hash, radon_error::RadonErrors};
use witnet_node::utils::stop_system_if_panicking;

/// DrReporter actor sends the the Witnet Request tally results to Ethereum
#[derive(Default)]
pub struct DrReporter {
    /// WRB contract
    pub wrb_contract: Option<Arc<Contract<web3::transports::Http>>>,
    /// eth_account
    pub eth_account: H160,
    /// report_result_limit
    pub report_result_limit: Option<u64>,
    /// maximum result size (in bytes)
    pub max_result_size: usize,
    /// Pending reportResult transactions. The actor should not attempt to report these requests
    /// until the timeout has elapsed
    pub pending_report_result: HashSet<DrId>,
    /// Max time to wait for an ethereum transaction to be confirmed before returning an error
    pub eth_confirmation_timeout_ms: u64,
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
    pub fn from_config(config: &Config, wrb_contract: Arc<Contract<Http>>) -> Self {
        Self {
            wrb_contract: Some(wrb_contract),
            eth_account: config.eth_account,
            report_result_limit: config.gas_limits.report_result,
            max_result_size: config.max_result_size,
            pending_report_result: Default::default(),
            eth_confirmation_timeout_ms: config.eth_confirmation_timeout_ms,
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
    /// Data request id in ethereum
    pub dr_id: DrId,
    /// Timestamp of the solving commit txs in Witnet. If zero is provided, EVM-timestamp will be used instead
    pub timestamp: u64,
    /// Hash of the data request in witnet
    pub dr_tx_hash: Hash,
    /// Data request result from witnet, in bytes
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
            if self.pending_report_result.contains(&report.dr_id) {
                // Timeout is not over yet, no action is needed
                log::debug!(
                    "Request [{}] is already being resolved, ignoring DrReporterMsg",
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
        let dr_ids2 = dr_ids.clone();
        let wrb_contract = self.wrb_contract.clone().unwrap();
        let eth_account = self.eth_account;
        let report_result_limit = self.report_result_limit;
        let eth_confirmation_timeout = Duration::from_millis(self.eth_confirmation_timeout_ms);

        for report in &mut msg.reports {
            if report.result.len() > self.max_result_size {
                let radon_error = RadonErrors::BridgeOversizedResult as u8;
                report.result = vec![0xD8, 0x27, 0x81, 0x18, radon_error]
            }
        }

        // New request or timeout elapsed, save dr_id
        for report in &msg.reports {
            self.pending_report_result.insert(report.dr_id);
        }

        let fut = async move {
            // Check if the request has already been resolved by some old pending transaction
            // that got confirmed after the eth_confirmation_timeout has elapsed
            let mut reports = vec![];
            for report in msg.reports.drain(..) {
                if let Some(set_dr_info_bridge_msg) =
                    read_resolved_request_from_contract(report.dr_id, &wrb_contract, eth_account)
                        .await
                {
                    // The request is already resolved, mark as resolved
                    let dr_database_addr = DrDatabase::from_registry();
                    dr_database_addr.send(set_dr_info_bridge_msg).await.ok();
                } else {
                    // Not resolved yet, insert back into the list
                    reports.push(report);
                }
            }
            msg.reports = reports;

            if msg.reports.is_empty() {
                // Nothing to report
                return;
            }

            let max_gas_price = get_max_gas_price(&msg, &wrb_contract, eth_account).await;

            if max_gas_price == U256::from(0u8) {
                // Error reading gas price, abort
                return;
            }

            let batch_results: Vec<_> = msg
                .reports
                .iter()
                .map(|report| {
                    let dr_hash = H256::from_slice(report.dr_tx_hash.as_ref());

                    // the trait `web3::contract::tokens::Tokenize` is not implemented for
                    // `(std::vec::Vec<(web3::types::U256, web3::types::U256, web3::types::H256, std::vec::Vec<u8>)>, bool)
                    // Need to manually convert to tuple
                    Token::Tuple(vec![
                        Token::Uint(report.dr_id),
                        Token::Uint(report.timestamp.into()),
                        Token::FixedBytes(dr_hash.to_fixed_bytes().to_vec()),
                        Token::Bytes(report.result.clone()),
                    ])
                })
                .collect();
            let verbose = true;
            let batches = split_by_gas_limit(
                batch_results,
                &wrb_contract,
                eth_account,
                report_result_limit,
                verbose,
                max_gas_price,
            )
            .await;

            log::debug!(
                "Requests [{:?}] will be reported in {} transactions",
                dr_ids,
                batches.len()
            );

            for (batch_results, estimated_gas_limit) in batches {
                if batch_results.len() > 1 {
                    log::debug!("Executing reportResultBatch {:?}", batch_results);
                } else {
                    log::debug!("Executing reportResult {:?}", batch_results);
                }
                let params_str;
                let only_1_batch = batch_results.len() == 1;
                let receipt = if only_1_batch {
                    let (dr_id, ts, dr_tx_hash, report_result) =
                        unwrap_batch(batch_results[0].clone());
                    params_str = format!(
                        "reportResult{:?}",
                        (&dr_id, &ts, &dr_tx_hash, &report_result)
                    );

                    let receipt_fut = wrb_contract.call_with_confirmations(
                        "reportResult",
                        (dr_id, ts, dr_tx_hash, report_result),
                        eth_account,
                        contract::Options::with(|opt| {
                            opt.gas = Some(estimated_gas_limit);
                            opt.gas_price = Some(max_gas_price);
                        }),
                        1,
                    );
                    tokio::time::timeout(eth_confirmation_timeout, receipt_fut).await
                } else {
                    params_str = format!("reportResultBatch{:?}", (&batch_results, verbose));

                    let receipt_fut = wrb_contract.call_with_confirmations(
                        "reportResultBatch",
                        (batch_results, verbose),
                        eth_account,
                        contract::Options::with(|opt| {
                            opt.gas = Some(estimated_gas_limit);
                            opt.gas_price = Some(max_gas_price);
                        }),
                        1,
                    );
                    tokio::time::timeout(eth_confirmation_timeout, receipt_fut).await
                };

                match receipt {
                    Ok(Ok(receipt)) => {
                        log::debug!("Request [{:?}], reportResult: {:?}", dr_ids, receipt);
                        match handle_receipt(&receipt).await {
                            Ok(()) => {
                                // TODO: set successful reports as Finished using SetDrInfoBridge message
                                // Need to detect which of the reports succeeded and which ones did not
                                log::debug!("{}: success", params_str);
                            }
                            Err(()) => {
                                log::error!("{}: transaction reverted (?)", params_str);
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        // Error in call_with_confirmations
                        log::error!("{}: {:?}", params_str, e);
                    }
                    Err(_e) => {
                        // Timeout is over
                        log::warn!("{}: timeout is over", params_str);
                    }
                }
            }
        };

        ctx.spawn(fut.into_actor(self).map(move |(), act, _ctx| {
            // Reset timeouts
            for dr_id in dr_ids2 {
                act.pending_report_result.remove(&dr_id);
            }
        }));
    }
}

/// Check if the request is already resolved in the WRB contract
async fn read_resolved_request_from_contract(
    dr_id: U256,
    wrb_contract: &Contract<Http>,
    eth_account: H160,
) -> Option<SetDrInfoBridge> {
    let query_status: Result<u8, web3::contract::Error> = wrb_contract
        .query(
            "getQueryStatus",
            (dr_id,),
            eth_account,
            contract::Options::default(),
            None,
        )
        .await;

    match query_status {
        Ok(status) => match WitnetQueryStatus::from_code(status) {
            WitnetQueryStatus::Unknown => log::debug!("[{}] does not exist, skipping", dr_id),
            WitnetQueryStatus::Posted => {
                log::debug!("[{}] has not got a result yet, skipping", dr_id)
            }
            WitnetQueryStatus::Reported => {
                log::debug!("[{}] already reported", dr_id);
                return process_reported_request(dr_id, wrb_contract, eth_account).await;
            }
            WitnetQueryStatus::Deleted => {
                log::debug!("[{}] already reported and deleted", dr_id);
                return Some(SetDrInfoBridge(
                    dr_id,
                    DrInfoBridge {
                        dr_bytes: Bytes::default(),
                        dr_state: DrState::Finished,
                        dr_tx_hash: None,
                        dr_tx_creation_timestamp: None,
                    },
                ));
            }
        },
        Err(err) => {
            log::error!(
                "Fail to read getQueryStatus from contract: {:?}",
                err.to_string(),
            );
        }
    }
    None
}

async fn get_max_gas_price(
    msg: &DrReporterMsg,
    wrb_contract: &Contract<Http>,
    eth_account: H160,
) -> U256 {
    // The gas price of the report transaction should equal the maximum gas price paid
    // by any of the requests being solved here
    let mut max_gas_price: U256 = U256::from(0u8);
    for report in &msg.reports {
        // Read gas price
        let dr_gas_price: Result<U256, web3::contract::Error> = wrb_contract
            .query(
                "readRequestGasPrice",
                report.dr_id,
                eth_account,
                contract::Options::default(),
                None,
            )
            .await;
        match dr_gas_price {
            Ok(dr_gas_price) => {
                max_gas_price = std::cmp::max(max_gas_price, dr_gas_price);
            }
            Err(e) => {
                log::error!("[{}] ReadGasPrice {:?}", report.dr_id, e);
                continue;
            }
        }
    }

    max_gas_price
}

/// Split batch_param (argument of reportResultBatch) into multiple smaller batch_param in order to
/// fit into the gas limit.
///
/// Returns a list of `(batch_param, estimated_gas)` that should be used to create
/// "reportResultBatch" transactions.
async fn split_by_gas_limit(
    batch_param: Vec<Token>,
    wrb_contract: &Contract<Http>,
    eth_account: H160,
    report_result_limit: Option<u64>,
    verbose: bool,
    max_gas_price: U256,
) -> Vec<(Vec<Token>, U256)> {
    let mut v = vec![];
    let mut stack = vec![batch_param];

    while let Some(batch_param) = stack.pop() {
        let params = (batch_param.clone(), verbose);
        let estimated_gas = wrb_contract
            .estimate_gas(
                "reportResultBatch",
                params,
                eth_account,
                contract::Options::with(|opt| {
                    opt.gas = report_result_limit.map(Into::into);
                    opt.gas_price = Some(max_gas_price);
                }),
            )
            .await;
        log::debug!(
            "reportResultBatch {} estimated gas: {:?}",
            batch_param.len(),
            estimated_gas
        );

        match estimated_gas {
            Ok(estimated_gas) => {
                v.push((batch_param, estimated_gas));
            }
            Err(e) => {
                if batch_param.len() <= 1 {
                    log::error!("reportResultBatch estimate gas: {:?}", e);
                    log::warn!("skipped dr: {:?}", batch_param);
                } else {
                    // Split batch_param in half
                    let (batch_param1, batch_param2) = batch_param.split_at(batch_param.len() / 2);
                    stack.push(batch_param1.to_vec());
                    stack.push(batch_param2.to_vec());
                }
            }
        }
    }

    v
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hack_fix_functions_with_multiple_definitions;
    use web3::contract::tokens::Tokenize;

    #[test]
    fn report_result_type_check() {
        let wrb_contract_abi_json: &[u8] = include_bytes!("../../wrb_abi.json");
        let mut wrb_contract_abi = web3::ethabi::Contract::load(wrb_contract_abi_json)
            .map_err(|e| format!("Unable to load WRB contract from ABI: {:?}", e))
            .unwrap();
        hack_fix_functions_with_multiple_definitions(&mut wrb_contract_abi);

        let msg = DrReporterMsg {
            reports: vec![Report {
                dr_id: DrId::from(4358u32),
                timestamp: 0,
                dr_tx_hash: Hash::SHA256([
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
                    Token::Uint(report.timestamp.into()),
                    Token::FixedBytes(dr_hash.to_fixed_bytes().to_vec()),
                    Token::Bytes(report.result.clone()),
                ])
            })
            .collect();
        let verbose = true;

        let params_one = unwrap_batch(batch_results[0].clone());
        wrb_contract_abi
            .function("reportResult")
            .and_then(|function| function.encode_input(&params_one.into_tokens()))
            .expect("encode args failed");

        let params_batch = (batch_results, verbose);
        wrb_contract_abi
            .function("reportResultBatch")
            .and_then(|function| function.encode_input(&params_batch.into_tokens()))
            .expect("encode args failed");
    }
}
