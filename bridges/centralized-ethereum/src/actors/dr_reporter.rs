use crate::actors::dr_database::{DrInfoBridge, SetDrInfoBridge};
use crate::{
    actors::dr_database::{DrDatabase, DrId, DrState},
    config::Config,
};
use actix::prelude::*;
use web3::{
    contract::{self, Contract},
    ethabi::Bytes,
    types::{H160, U256},
};
use witnet_data_structures::chain::Hash;

/// EthPoller (TODO: Explanation)
#[derive(Default)]
pub struct DrReporter {
    /// WRB contract
    pub wrb_contract: Option<Contract<web3::transports::Http>>,
    /// eth_account
    pub eth_account: H160,
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
        let web3_http = web3::transports::Http::new(&config.eth_client_url)
            .map_err(|e| format!("Failed to connect to Ethereum client.\nError: {:?}", e))?;
        let web3 = web3::Web3::new(web3_http);
        // Why read files at runtime when you can read files at compile time
        let wrb_contract_abi_json: &[u8] = include_bytes!("../../wrb_abi.json");
        let wrb_contract_abi = web3::ethabi::Contract::load(wrb_contract_abi_json)
            .map_err(|e| format!("Unable to load WRB contract from ABI: {:?}", e))?;
        let wrb_contract_address = config.wrb_contract_addr;
        let wrb_contract = Contract::new(web3.eth(), wrb_contract_address, wrb_contract_abi);

        Ok(Self {
            wrb_contract: Some(wrb_contract),
            eth_account: config.eth_account,
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

    fn handle(&mut self, msg: DrReporterMsg, ctx: &mut Self::Context) -> Self::Result {
        // TODO: create ethereum transaction and set database state to finished
        let wrb_contract = self.wrb_contract.clone().unwrap();
        let eth_account = self.eth_account;
        let params_str = format!("{:?}", &(msg.dr_id, msg.dr_tx_hash, msg.result.clone()));
        let dr_hash: U256 = match msg.dr_tx_hash {
            Hash::SHA256(x) => x.into(),
        };

        let fut = async move {
            let receipt = wrb_contract
                .call_with_confirmations(
                    "reportResult",
                    (msg.dr_id, dr_hash, msg.result),
                    eth_account,
                    // contract::Options::with(|opt| {
                    //     opt.gas = config.gas_limits.report_result.map(Into::into);
                    //     opt.gas_price = Some(gas_price);
                    // }
                    contract::Options::default(),
                    1,
                )
                .await;
            match receipt {
                Ok(tx) => {
                    log::debug!("reportResult: {:?}", tx);
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
        };
        ctx.spawn(fut.into_actor(self));
    }
}
