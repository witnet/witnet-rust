use crate::{
    actors::dr_database::{DrDatabase, DrId, DrInfoBridge, DrState, SetDrInfoBridge},
    config::Config,
    create_wrb_contract,
};
use actix::prelude::*;
use ethabi::Bytes;
use web3::{
    contract::{self, Contract},
    types::{H160, U256},
};
use witnet_data_structures::{chain::Hash, radon_error::RadonErrors};

/// DrReporter actor sends the the Witnet Request tally results to Ethereum
#[derive(Default)]
pub struct DrReporter {
    /// WRB contract
    pub wrb_contract: Option<Contract<web3::transports::Http>>,
    /// eth_account
    pub eth_account: H160,
    /// report_result_limit
    pub report_result_limit: Option<u64>,
    /// maximum result size (in bytes)
    pub max_result_size: usize,
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
    pub fn from_config(config: &Config) -> Result<Self, String> {
        let wrb_contract = create_wrb_contract(config);

        Ok(Self {
            wrb_contract: Some(wrb_contract),
            eth_account: config.eth_account,
            report_result_limit: config.gas_limits.report_result,
            max_result_size: config.max_result_size,
        })
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
        let wrb_contract = self.wrb_contract.clone().unwrap();
        let eth_account = self.eth_account;
        let report_result_limit = self.report_result_limit;
        let params_str = format!("{:?}", &(msg.dr_id, msg.dr_tx_hash, msg.result.clone()));
        let dr_hash: U256 = match msg.dr_tx_hash {
            Hash::SHA256(x) => x.into(),
        };

        if msg.result.len() > self.max_result_size {
            let radon_error = RadonErrors::BridgeOversizedResult as u8;
            msg.result = vec![0xD8, 0x27, 0x81, 0x18, radon_error]
        }

        let fut = async move {
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
                    let receipt = wrb_contract
                        .call_with_confirmations(
                            "reportResult",
                            (msg.dr_id, dr_hash, msg.result),
                            eth_account,
                            contract::Options::with(|opt| {
                                opt.gas = report_result_limit.map(Into::into);
                                opt.gas_price = Some(dr_gas_price);
                            }),
                            1,
                        )
                        .await;
                    match receipt {
                        Ok(tx) => {
                            log::debug!("Request [{}], reportResult: {:?}", msg.dr_id, tx);
                            let dr_database_addr = DrDatabase::from_registry();

                            dr_database_addr
                                .send(SetDrInfoBridge(
                                    msg.dr_id,
                                    DrInfoBridge {
                                        dr_bytes: msg.dr_bytes,
                                        dr_state: DrState::Finished,
                                        dr_tx_hash: Some(msg.dr_tx_hash),
                                    },
                                ))
                                .await
                                .ok();
                        }
                        Err(e) => {
                            log::error!("reportResult{:?}: {:?}", params_str, e);
                        }
                    }
                }
                Err(e) => {
                    log::error!("ReadGasPrice {:?}", e);
                }
            }
        };

        // Wait here to only allow to report one data request at a time to prevent reporting the
        // same data request more than once.
        ctx.wait(fut.into_actor(self));
    }
}
