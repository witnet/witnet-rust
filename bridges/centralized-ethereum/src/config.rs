//! Configuration

use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};
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
    /// Address of a Request example deployed contract
    pub request_example_contract_addr: H160,
    /// Ethereum account used to create the transactions
    pub eth_account: H160,
    /// Period to check for new requests in the WRB
    pub eth_new_dr_polling_rate_ms: u64,
    /// Period to check for completed requests in Witnet
    pub wit_tally_polling_rate_ms: u64,
    /// Period to post new requests to Witnet
    pub wit_dr_sender_polling_rate_ms: u64,
    /// If the data request has been sent to witnet but it is not included in a block, retry after this many milliseconds
    pub dr_tx_unresolved_timeout_ms: Option<u64>,
    /// Max value that will be accepted by the bridge node in a data request
    pub max_dr_value_nanowits: u64,
    /// Running in the witnet testnet?
    pub witnet_testnet: bool,
    /// Gas limits for some methods. If missing, let the client estimate
    pub gas_limits: Gas,
    /// Storage
    pub storage: Storage,
    /// Maximum data request result size (in bytes)
    pub max_result_size: usize,
    /// Max time to wait for an ethereum transaction to be confirmed before returning an error
    pub eth_confirmation_timeout_ms: u64,
    /// Number of block confirmations needed to assume finality when sending transactions to ethereum
    #[serde(default = "one")]
    pub num_confirmations: usize,
}

fn one() -> usize {
    1
}

/// Gas limits for some methods. If missing, let the client estimate
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gas {
    /// postDataRequest gas limit
    pub post_data_request: Option<u64>,
    /// reportResult gas limit
    pub report_result: Option<u64>,
}

/// Storage
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Storage {
    /// Path to the directory that will contain the database. Used
    /// only if backend is RocksDB.
    pub db_path: PathBuf,
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
