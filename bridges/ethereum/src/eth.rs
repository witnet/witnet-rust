use crate::config::Config;
use ethabi::Token;
use futures::Future;
use log::*;
use web3::{
    contract::Contract,
    types::{H160, H256, U256},
};

/// State needed to interact with the ethereum side of the bridge
pub struct EthState {
    /// Web3 event loop handle
    pub _eloop: web3::transports::EventLoopHandle,
    /// Web3
    pub web3: web3::Web3<web3::transports::Http>,
    /// Accounts
    pub accounts: Vec<H160>,
    /// WBI contract
    pub wbi_contract: Contract<web3::transports::Http>,
    /// PostDataRequest event signature
    pub post_dr_event_sig: H256,
    /// InclusionDataRequest event signature
    pub inclusion_dr_event_sig: H256,
    /// PostResult event signature
    pub post_tally_event_sig: H256,
    /// BlockRelay contract
    pub block_relay_contract: Contract<web3::transports::Http>,
}

impl EthState {
    /// Read addresses from config and create `State` struct
    pub fn create(config: &Config) -> Result<Self, ()> {
        let (_eloop, web3_http) =
            web3::transports::Http::new(&config.eth_client_url).map_err(|_| ())?;
        let web3 = web3::Web3::new(web3_http);
        let accounts = web3.eth().accounts().wait().map_err(|_| ())?;
        debug!("Web3 accounts: {:?}", accounts);

        // Why read files at runtime when you can read files at compile time
        let wbi_contract_abi_json: &[u8] = include_bytes!("../wbi_abi.json");
        let wbi_contract_abi = ethabi::Contract::load(wbi_contract_abi_json).map_err(|_| ())?;
        let wbi_contract_address = config.wbi_contract_addr;
        let wbi_contract =
            Contract::new(web3.eth(), wbi_contract_address, wbi_contract_abi.clone());

        let block_relay_contract_abi_json: &[u8] = include_bytes!("../block_relay_abi.json");
        let block_relay_contract_abi =
            ethabi::Contract::load(block_relay_contract_abi_json).map_err(|_| ())?;
        let block_relay_contract_address = config.block_relay_contract_addr;
        let block_relay_contract = Contract::new(
            web3.eth(),
            block_relay_contract_address,
            block_relay_contract_abi.clone(),
        );

        //debug!("WBI events: {:?}", contract_abi.events);
        let post_dr_event = wbi_contract_abi
            .event("PostDataRequest")
            .map_err(|_| ())?
            .clone();
        let inclusion_dr_event = wbi_contract_abi
            .event("InclusionDataRequest")
            .map_err(|_| ())?
            .clone();
        let post_tally_event = wbi_contract_abi
            .event("PostResult")
            .map_err(|_| ())?
            .clone();

        let post_dr_event_sig = post_dr_event.signature();
        let inclusion_dr_event_sig = inclusion_dr_event.signature();
        let post_tally_event_sig = post_tally_event.signature();

        Ok(Self {
            _eloop,
            web3,
            accounts,
            wbi_contract,
            block_relay_contract,
            post_dr_event_sig,
            inclusion_dr_event_sig,
            post_tally_event_sig,
        })
    }
}

/// Assume the first return value of an event log is a U256 and return it
pub fn read_u256_from_event_log(value: &web3::types::Log) -> Result<U256, ()> {
    let event_types = vec![ethabi::ParamType::Uint(0)];
    let event_data = ethabi::decode(&event_types, &value.data.0);
    debug!("Event data: {:?}", event_data);

    match event_data.map_err(|_| ())?.get(0).ok_or(())? {
        Token::Uint(x) => Ok(*x),
        _ => Err(()),
    }
}

/// Possible ethereum events emited by the WBI ethereum contract
pub enum WbiEvent {
    /// A new data request has been posted to ethereum
    PostDataRequest(U256),
    /// A data request from ethereum has been posted to witnet with a proof of
    /// inclusion in a block
    InclusionDataRequest(U256),
    /// A data request has been resolved in witnet, and the result was reported
    /// to ethereum with a proof of inclusion
    PostResult(U256),
}
