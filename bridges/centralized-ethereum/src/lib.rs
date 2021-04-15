//! Witnet <> Ethereum bridge
#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

use crate::config::Config;
use web3::{contract::Contract, transports::Http};

/// Actors
pub mod actors;
/// Configuration
pub mod config;

/// Creates a Witnet Request Board contract from Config information
pub fn create_wrb_contract(config: &Config) -> Contract<Http> {
    let web3_http = web3::transports::Http::new(&config.eth_client_url)
        .map_err(|e| format!("Failed to connect to Ethereum client.\nError: {:?}", e))
        .unwrap();
    let web3 = web3::Web3::new(web3_http);
    // Why read files at runtime when you can read files at compile time
    let wrb_contract_abi_json: &[u8] = include_bytes!("../wrb_abi.json");
    let wrb_contract_abi = web3::ethabi::Contract::load(wrb_contract_abi_json)
        .map_err(|e| format!("Unable to load WRB contract from ABI: {:?}", e))
        .unwrap();
    let wrb_contract_address = config.wrb_contract_addr;
    Contract::new(web3.eth(), wrb_contract_address, wrb_contract_abi)
}
