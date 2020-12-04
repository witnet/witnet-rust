//! Configuration

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::Path;
use web3::types::H160;
use witnet_data_structures::chain::Environment;

/// Configuration
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Address of the witnet node JSON-RPC server
    pub witnet_jsonrpc_addr: SocketAddr,
    /// Url of the ethereum client
    pub eth_client_url: String,
    /// Address of the WitnetRequestsBoard deployed contract
    pub wrb_contract_addr: H160,
    /// Address of the BlockRelay deployed contract
    pub block_relay_contract_addr: H160,
    /// Ethereum account used to create the transactions
    pub eth_account: H160,
    /// Enable block relay from witnet to ethereum, relay only new blocks
    /// (blocks that were recently consolidated)
    pub enable_block_relay_new_blocks: bool,
    /// Enable block relay from witnet to ethereum, relay only old blocks
    /// (old blocks that were never posted to the block relay)
    pub enable_block_relay_old_blocks: bool,
    /// Relay all superblocks
    pub relay_all_superblocks_even_the_empty_ones: bool,
    /// Enable data request claim + inclusion
    pub enable_claim_and_inclusion: bool,
    /// Enable data request result reporting
    pub enable_result_reporting: bool,
    /// If post_to_witnet_more_than_once is enabled, this is the minimum time in seconds that must
    /// elapse before the same data request is created and broadcasted to the Witnet network.
    pub post_to_witnet_again_after_timeout: u64,
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
    /// Period to check for state updates in existing requests in the WRB
    pub eth_existing_dr_polling_rate_ms: u64,
    /// Period to check for new requests in the WRB
    pub eth_new_dr_polling_rate_ms: u64,
    /// Running in the witnet testnet?
    pub witnet_testnet: bool,
    /// If readDrHash returns 0, try again later
    pub read_dr_hash_interval_ms: u64,
    /// Gas limits for some methods. If missing, let the client estimate
    pub gas_limits: Gas,
}

/// Gas limits for some methods. If missing, let the client estimate
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gas {
    /// claimDataRequests gas limit
    pub claim_data_requests: Option<u64>,
    /// postDataRequest gas limit
    pub post_data_request: Option<u64>,
    /// postNewBlock gas limit
    pub post_new_block: Option<u64>,
    /// reportDataRequestInclusion gas limit
    pub report_data_request_inclusion: Option<u64>,
    /// reportResult gas limit
    pub report_result: Option<u64>,
}

/// Load configuration from a file written in Toml format.
pub fn from_file<S: AsRef<Path>>(file: S) -> Result<Config, Box<dyn std::error::Error>> {
    use std::fs::File;
    use std::io::Read;

    let f = file.as_ref();
    let mut contents = String::new();

    log::debug!("Loading config from `{}`", f.to_string_lossy());

    let mut file = File::open(file)?;
    file.read_to_string(&mut contents)?;
    let c: Config = toml::from_str(&contents)?;
    // Set environment: must be the same as the witnet node
    witnet_data_structures::set_environment(if c.witnet_testnet {
        Environment::Testnet
    } else {
        Environment::Mainnet
    });

    Ok(c)
}
