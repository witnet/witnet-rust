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
    //TODO: unimplemented
    pub eth_account: H160,
    /// Enable block relay from witnet to ethereum?
    pub enable_block_relay: bool,
}

/// Load configuration from a file written in Toml format.
pub fn from_file<S: AsRef<Path>>(file: S) -> Result<Config, toml::de::Error> {
    use std::fs::File;
    use std::io::Read;

    let f = file.as_ref();
    let mut contents = String::new();

    debug!("Loading config from `{}`", f.to_string_lossy());

    let mut file = File::open(file).unwrap();
    file.read_to_string(&mut contents).unwrap();
    toml::from_str(&contents)
}

/// Read config from default file
pub fn read_config() -> Result<Config, toml::de::Error> {
    from_file("witnet_ethereum_bridge.toml")
}
