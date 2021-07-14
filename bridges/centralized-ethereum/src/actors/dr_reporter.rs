use crate::{
    actors::dr_database::{DrDatabase, DrId, DrInfoBridge, DrState, SetDrInfoBridge},
    config::Config,
    handle_receipt,
};
use actix::prelude::*;
use std::{collections::HashSet, sync::Arc, time::Duration};
use web3::{
    contract::{self, Contract},
    ethabi::Bytes,
    transports::Http,
    types::{H160, U256},
};
use witnet_data_structures::{chain::Hash, radon_error::RadonErrors};
use witnet_util::timestamp::get_timestamp;

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

/// Report the result of this data request id to ethereum
pub struct DrReporterMsg {
    /// Data request id in ethereum
    pub dr_id: DrId,
    /// Data Request Bytes
    pub dr_bytes: Bytes,
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
        if self.pending_report_result.contains(&msg.dr_id) {
            // Timeout not elapsed, abort
            log::debug!(
                "Request [{}] is already being resolved, ignoring DrReporterMsg",
                msg.dr_id
            );
            return;
        }

        let dr_id = msg.dr_id;
        let wrb_contract = self.wrb_contract.clone().unwrap();
        let eth_account = self.eth_account;
        let report_result_limit = self.report_result_limit;
        let eth_confirmation_timeout = Duration::from_millis(self.eth_confirmation_timeout_ms);
        let params_str = format!("{:?}", &(msg.dr_id, msg.dr_tx_hash, msg.result.clone()));
        let dr_hash: U256 = match msg.dr_tx_hash {
            Hash::SHA256(x) => x.into(),
        };

        if msg.result.len() > self.max_result_size {
            let radon_error = RadonErrors::BridgeOversizedResult as u8;
            msg.result = vec![0xD8, 0x27, 0x81, 0x18, radon_error]
        }

        // New request or timeout elapsed, save dr_id
        self.pending_report_result.insert(msg.dr_id);

        let fut = async move {
            // Check if the request has already been resolved by some old pending transaction
            // that got confirmed after the eth_confirmation_timeout has elapsed
            if let Some(set_dr_info_bridge_msg) =
                read_resolved_request_from_contract(msg.dr_id, &wrb_contract, eth_account).await
            {
                let dr_database_addr = DrDatabase::from_registry();
                dr_database_addr.send(set_dr_info_bridge_msg).await.ok();
                // The request is already resolved, nothing more to do
                return;
            }

            // Report result
            let dr_gas_price: Result<U256, web3::contract::Error> = wrb_contract
                .query(
                    "readGasPrice",
                    msg.dr_id,
                    eth_account,
                    contract::Options::default(),
                    None,
                )
                .await;

            match dr_gas_price {
                Ok(dr_gas_price) => {
                    log::debug!("Request [{}], calling reportResult", msg.dr_id);
                    let receipt_fut = wrb_contract.call_with_confirmations(
                        "reportResult",
                        (msg.dr_id, dr_hash, msg.result),
                        eth_account,
                        contract::Options::with(|opt| {
                            opt.gas = report_result_limit.map(Into::into);
                            opt.gas_price = Some(dr_gas_price);
                        }),
                        1,
                    );
                    let receipt = tokio::time::timeout(eth_confirmation_timeout, receipt_fut).await;
                    match receipt {
                        Ok(Ok(receipt)) => {
                            log::debug!("Request [{}], reportResult: {:?}", msg.dr_id, receipt);
                            match handle_receipt(receipt).await {
                                Ok(()) => {
                                    let dr_database_addr = DrDatabase::from_registry();

                                    dr_database_addr
                                        .send(SetDrInfoBridge(
                                            msg.dr_id,
                                            DrInfoBridge {
                                                dr_bytes: msg.dr_bytes,
                                                dr_state: DrState::Finished,
                                                dr_tx_hash: Some(msg.dr_tx_hash),
                                                dr_tx_creation_timestamp: Some(get_timestamp()),
                                            },
                                        ))
                                        .await
                                        .ok();
                                }
                                Err(()) => {
                                    log::error!(
                                        "reportResult{:?}: transaction reverted (?)",
                                        params_str
                                    );
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            // Error in call_with_confirmations
                            log::error!("reportResult{:?}: {:?}", params_str, e);
                        }
                        Err(_e) => {
                            // Timeout elapsed
                            log::warn!("reportResult{:?}: timeout elapsed", params_str);
                        }
                    }
                }
                Err(e) => {
                    log::error!("ReadGasPrice {:?}", e);
                }
            }
        };

        ctx.spawn(fut.into_actor(self).map(move |(), act, _ctx| {
            // Reset timeout
            act.pending_report_result.remove(&dr_id);
        }));
    }
}

/// Check if the request is already resolved in the WRB contract
async fn read_resolved_request_from_contract(
    dr_id: U256,
    wrb_contract: &Contract<Http>,
    eth_account: H160,
) -> Option<SetDrInfoBridge> {
    match wrb_contract
        .query(
            "readDrTxHash",
            (dr_id,),
            eth_account,
            contract::Options::default(),
            None,
        )
        .await
    {
        Err(e) => {
            log::warn!(
                "[{}] readDrTxHash error, assuming that the request is not resolved yet: {:?}",
                dr_id,
                e
            );
        }
        Ok(dr_tx_hash) => {
            let dr_tx_hash: U256 = dr_tx_hash;
            if dr_tx_hash != U256::from(0u8) {
                // Non-zero data request transaction hash: this data request is already "Finished"
                log::debug!("[{}] already finished", dr_id);

                match wrb_contract
                    .query(
                        "readDataRequest",
                        (dr_id,),
                        eth_account,
                        contract::Options::default(),
                        None,
                    )
                    .await
                {
                    Err(e) => {
                        log::warn!("[{}] readDataRequest error, assuming that the request is not resolved yet: {:?}", dr_id, e);
                    }
                    Ok(dr_bytes) => {
                        log::debug!("[{}] was already resolved", dr_id);
                        return Some(SetDrInfoBridge(
                            dr_id,
                            DrInfoBridge {
                                dr_bytes,
                                dr_state: DrState::Finished,
                                dr_tx_hash: Some(Hash::SHA256(dr_tx_hash.into())),
                                dr_tx_creation_timestamp: Some(get_timestamp()),
                            },
                        ));
                    }
                }
            }
        }
    }

    None
}
