//! Configuration

use log::*;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::Path;
use web3::types::H160;

/// Configuration
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Address of the witnet node JSON-RPC server
    pub witnet_jsonrpc_addr: SocketAddr,
    /// Url of the ethereum client
    pub eth_client_url: String,
    /// Address of the WitnetBridgeInterface deployed contract
    pub wbi_contract_addr: H160,
    /// Address of the BlockRelay deployed contract
    pub block_relay_contract_addr: H160,
    /// Ethereum account used to create the transactions
    pub eth_account: H160,
    /// Enable block relay from witnet to ethereum?
    pub enable_block_relay: bool,
    /// Enable data request claim + inclusion
    pub enable_claim_and_inclusion: bool,
    /// Enable data request result reporting
    pub enable_result_reporting: bool,
    /// Post data request more than once? Useful to retry if the data request
    /// was not included in a block
    pub post_to_witnet_more_than_once: bool,
    /// Subscribe to witnet blocks? This is only necessary for block relay
    pub subscribe_to_witnet_blocks: bool,
    /// Period to check for new blocks in block relay
    pub block_relay_polling_rate_ms: u64,
    /// Period to check for resolved data request using the witnet `dataRequestReport`
    /// method
    pub witnet_dr_report_polling_rate_ms: u64,
    /// Period to try to claim old data request whose claim expired
    pub claim_dr_rate_ms: u64,
    /// Period to check for new Ethereum events
    pub eth_event_polling_rate_ms: u64,
}

/// Load configuration from a file written in Toml format.
pub fn from_file<S: AsRef<Path>>(file: S) -> Result<Config, Box<dyn std::error::Error>> {
    use std::fs::File;
    use std::io::Read;

    let f = file.as_ref();
    let mut contents = String::new();

    debug!("Loading config from `{}`", f.to_string_lossy());

    let mut file = File::open(file)?;
    file.read_to_string(&mut contents)?;
    Ok(toml::from_str(&contents)?)
}
