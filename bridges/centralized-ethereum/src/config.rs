//! Configuration

use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use std::{
    cell::Cell,
    net::SocketAddr,
    path::{Path, PathBuf},
};
use web3::types::H160;
use witnet_data_structures::chain::Environment;

/// Configuration
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Ethereum account used to report data request results
    pub eth_from: H160,
    /// Ethereum account balance under which alerts will be logged
    pub eth_from_balance_threshold: u64,
    /// Gas limits for some methods. If missing, let the client estimate
    #[serde(deserialize_with = "nested_toml_if_using_envy")]
    pub eth_gas_limits: Gas,
    /// Url of the ethereum client
    pub eth_jsonrpc_url: String,
    /// Max number of queries to be batched together
    #[serde(default = "default_max_batch_size")]
    pub eth_max_batch_size: u16,
    /// Price of $nanoWit in Wei, used to improve estimation of report profits
    pub eth_nanowit_wei_price: Option<u64>,
    /// Polling period for checking new queries in the WitnetOracle contract
    pub eth_new_drs_polling_rate_ms: u64,
    /// Number of block confirmations needed to assume finality when sending transactions to ethereum
    #[serde(default = "one")]
    pub eth_txs_confirmations: usize,
    /// Max time to wait for an ethereum transaction to be confirmed before returning an error
    pub eth_txs_timeout_ms: u64,
    /// Address of the WitnetRequestsBoard contract
    pub eth_witnet_oracle: H160,

    /// Let the dog out?
    pub watch_dog_enabled: bool,
    /// Watch dog polling rate
    #[serde(default = "default_watch_dog_polling_rate_ms")]
    pub watch_dog_polling_rate_ms: u64,

    /// Minimum collateral required on data requests read from the WitnetOracle contract
    pub witnet_dr_min_collateral_nanowits: u64,
    /// Maximium data request transaction fee assumed by the bridge
    pub witnet_dr_max_fee_nanowits: u64,
    /// Maximum data request result size (in bytes) will accept to report
    pub witnet_dr_max_result_size: usize,
    /// Maximum data request value that the bridge will accept to relay
    pub witnet_dr_max_value_nanowits: u64,
    /// Polling period for checking resolution of data requests in the Witnet blockchain
    pub witnet_dr_txs_polling_rate_ms: u64,
    /// Max time to wait for data request resolutions, in milliseconds
    pub witnet_dr_txs_timeout_ms: u64,
    /// Address of the witnet node JSON-RPC server
    pub witnet_jsonrpc_socket: SocketAddr,
    /// Running in the witnet testnet?
    pub witnet_testnet: bool,
    /// Bridge UTXO min value threshold
    pub witnet_utxo_min_value_threshold: u64,

    /// Storage
    #[serde(deserialize_with = "nested_toml_if_using_envy")]
    pub storage: Storage,
    /// Skip first requests up to index n when updating database
    pub storage_skip_first: Option<u64>,
}

fn one() -> usize {
    1
}

fn default_max_batch_size() -> u16 {
    256
}

fn default_watch_dog_polling_rate_ms() -> u64 {
    900_000
}

/// Gas limits for some methods. If missing, let the client estimate
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gas {
    /// postDataRequest gas limit
    pub post_data_request: Option<u64>,
    /// reportResult gas limit
    pub report_result: Option<u64>,
}

/// Storage
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
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

/// Load configuration from environment variables
pub fn from_env() -> Result<Config, envy::Error> {
    USING_ENVY.with(|x| x.set(true));
    let res = envy::prefixed("WITNET_CENTRALIZED_ETHEREUM_BRIDGE_").from_env();
    USING_ENVY.with(|x| x.set(false));

    res
}

thread_local! {
    /// Thread-local flag to indicate the `nested_toml_if_using_envy` function that we are indeed
    /// using envy.
    static USING_ENVY: Cell<bool> = const { Cell::new(false) };
}

/// If using the `envy` crate to deserialize this value, try to deserialize it as a TOML string.
/// If using any other deserializer, deserialize the value as usual.
///
/// The thread-local variable `USING_ENVY` is used to detect which deserializer is currently being
/// used.
fn nested_toml_if_using_envy<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    if USING_ENVY.with(|x| x.get()) {
        // Trying to deserialize a `&'de str` here fails with error:
        //   invalid type: string \"\", expected a borrowed string
        // because the envy crate only supports deserializing strings.
        // So instead we deserialize into a `String` and leak that string to get a `&'static str`.
        // TODO: find a better way to get a &'de str
        // Maybe just use static storage to store the 2 strings, like a [Option<String>; 2], but
        // that is basically the same as leaking the strings.
        let string_toml = String::deserialize(deserializer)?;
        let s = Box::leak(string_toml.into_boxed_str());

        toml::from_str(s).map_err(D::Error::custom)
    } else {
        T::deserialize(deserializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envy_deserialize_nested_toml() {
        // The envy crate does not support deserializing nested structs, such as the `Gas` struct
        // inside `Config`. As a workaround, we add the attribute
        // #[serde(deserialize_with = "nested_toml_if_using_envy")]
        // which will treat the string as toml, and allow a successful deserialization.

        // Copy of `Config` that only has the fields that are interesting for this test
        #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct SmallConfig {
            /// Gas limits for some methods. If missing, let the client estimate
            #[serde(deserialize_with = "nested_toml_if_using_envy")]
            pub eth_gas_limits: Gas,
            /// Storage
            #[serde(deserialize_with = "nested_toml_if_using_envy")]
            pub storage: Storage,
        }

        // kev-value list of environment variables
        let kv = vec![
            (
                "WITNET_CENTRALIZED_ETHEREUM_BRIDGE_GAS_LIMITS".to_string(),
                "post_data_request = 10_000\nreport_result = 20_000".to_string(),
            ),
            (
                "WITNET_CENTRALIZED_ETHEREUM_BRIDGE_STORAGE".to_string(),
                "db_path = \".witnet\"".to_string(),
            ),
        ];

        let expected = SmallConfig {
            eth_gas_limits: Gas {
                post_data_request: Some(10_000),
                report_result: Some(20_000),
            },
            storage: Storage {
                db_path: PathBuf::from(".witnet"),
            },
        };

        // Need to manually set the "USING_ENVY" flag, this is handled automatically inside the
        // from_env function which is the public one that users can use.
        USING_ENVY.with(|x| x.set(true));
        let small_config: SmallConfig = envy::prefixed("WITNET_CENTRALIZED_ETHEREUM_BRIDGE_")
            .from_iter(kv)
            .unwrap();
        USING_ENVY.with(|x| x.set(false));

        assert_eq!(small_config, expected);
    }
}
