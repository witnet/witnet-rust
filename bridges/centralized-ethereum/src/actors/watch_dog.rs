use crate::{
    actors::dr_database::{CountDrsPerState, DrDatabase},
    config::Config,
};
use actix::prelude::*;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use web3::{
    contract::Contract,
    transports::Http,
    types::H160,
};
use witnet_net::client::tcp::{jsonrpc, JsonRpcClient};
use witnet_node::utils::stop_system_if_panicking;

/// EthPoller actor reads periodically new requests from the WRB Contract and includes them
/// in the DrDatabase
#[derive(Default)]
pub struct WatchDog {
    /// JSON WIT/RPC client connection to Wit/node
    pub wit_client: Option<Addr<JsonRpcClient>>,
    /// JSON WIT/RPC socket address
    pub wit_jsonrpc_socket: String,
    /// Bridge UTXO min value threshold
    pub wit_utxo_min_value_threshold: u64,
    /// Web3 object
    pub eth_jsonrpc_url: String,
    /// Web3 signer address
    pub eth_account: H160,
    /// WitOracle bridge contract
    pub eth_contract: Option<Arc<Contract<web3::transports::Http>>>,
    /// Polling period for global status
    pub polling_rate_ms: u64,
    /// Instant at which the actor is created
    pub start_ts: Option<Instant>,
    /// Eth balance upon first metric report:
    pub start_eth_balance: Option<f64>,
    /// Wit balance upon last refund
    pub start_wit_balance: Option<f64>,
}

impl Drop for WatchDog {
    fn drop(&mut self) {
        log::trace!("Dropping WatchDog");
        stop_system_if_panicking("WatchDog");
    }
}

/// Make actor from EthPoller
impl Actor for WatchDog {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("WatchDog actor has been started!");

        self.watch_global_status(None, None, ctx, Duration::from_millis(self.polling_rate_ms));
    }
}

#[derive(Debug, PartialEq)]
enum WatchDogStatus {
    EvmDisconnect,
    EvmErrors,
    EvmSyncing,
    WitAlmostSynced,
    WitErrors,
    WitDisconnect,
    WitSyncing,
    WitWaitingConsensus,
    UpAndRunning
}

impl WatchDogStatus {
    fn to_string(&self) -> String {
        match self {
            WatchDogStatus::EvmDisconnect => "evm-disconnect".to_string(),
            WatchDogStatus::EvmErrors => format!("evm-errors"),
            WatchDogStatus::EvmSyncing => "evm-syncing".to_string(),
            WatchDogStatus::WitAlmostSynced => "wit-almost-synced".to_string(),
            WatchDogStatus::WitDisconnect => "wit-disconnect".to_string(),
            WatchDogStatus::WitErrors => format!("wit-errors"),
            WatchDogStatus::WitSyncing => "wit-syncing".to_string(),
            WatchDogStatus::WitWaitingConsensus => "wit-waiting-consensus".to_string(),            
            WatchDogStatus::UpAndRunning => "up-and-running".to_string(),
        }
    }
}

/// Required trait for being able to retrieve WatchDog address from system registry
impl actix::Supervised for WatchDog {}
impl SystemService for WatchDog {}

impl WatchDog {
    /// Initialize from config
    pub fn from_config(config: &Config, eth_contract: Arc<Contract<Http>>) -> Self {
        Self {
            wit_client: JsonRpcClient::start(config.witnet_jsonrpc_socket.to_string().as_str()).ok(),
            wit_jsonrpc_socket: config.witnet_jsonrpc_socket.to_string(),
            wit_utxo_min_value_threshold: config.witnet_utxo_min_value_threshold,
            eth_account: config.eth_from,
            eth_contract: Some(eth_contract),
            eth_jsonrpc_url: config.eth_jsonrpc_url.clone(),
            polling_rate_ms: config.watch_dog_polling_rate_ms,
            start_ts: Some(Instant::now()),
            start_eth_balance: None,
            start_wit_balance: None,
        }
    }

    fn watch_global_status(
        &mut self,
        eth_balance: Option<f64>,
        wit_balance: Option<f64>,
        ctx: &mut Context<Self>,
        period: Duration,
    ) {
        if self.start_eth_balance.is_none() && eth_balance.is_some() {
            self.start_eth_balance = eth_balance;
        }
        if let Some(wit_balance) = wit_balance {
            if wit_balance > self.start_wit_balance.unwrap_or_default() {
                self.start_wit_balance = Some(wit_balance);
                log::warn!("Wit account refunded to {} $WIT", wit_balance);
            }
        }
        let start_eth_balance = self.start_eth_balance;
        let start_wit_balance = self.start_wit_balance;
        let wit_client = self.wit_client.clone();
        let wit_jsonrpc_socket = self.wit_jsonrpc_socket.clone();
        let wit_utxo_min_value_threshold = self.wit_utxo_min_value_threshold;
        let eth_jsonrpc_url = self.eth_jsonrpc_url.clone();
        let eth_account = self.eth_account;
        let eth_contract_address = self.eth_contract.clone().unwrap().address();
        let running_secs = self.start_ts.unwrap().elapsed().as_secs();

        let fut = async move {
            let mut status = WatchDogStatus::UpAndRunning;

            let dr_database = DrDatabase::from_registry();
            let (_, drs_pending, drs_finished, _) =
                dr_database.send(CountDrsPerState).await.unwrap().unwrap();

            let mut metrics: String = "{".to_string();
            metrics.push_str(&format!("\"drsFinished\": {drs_finished}, "));
            metrics.push_str(&format!("\"drsPending\": {drs_pending}, "));
            metrics.push_str(&format!("\"evmAccount\": \"{eth_account}\", "));

            if let Some(wit_client) = wit_client {
                if let Err(err) = check_wit_connection_status(&wit_client).await {
                    status = err;
                }
    
                let (wit_account, wit_balance, wit_utxos_above_threshold) = match fetch_wit_info(
                    &wit_client,
                    wit_utxo_min_value_threshold
                ).await {
                    Ok((wit_account, wit_balance, wit_utxos_above_threshold)) => {
                        (wit_account, wit_balance, wit_utxos_above_threshold)
                    }
                    Err(err) => {
                        if status == WatchDogStatus::UpAndRunning {
                            status = err;
                        }
                        (None, None, None)
                    }
                };

                if wit_account.is_some() {
                    metrics.push_str(&format!("\"witAccount\": {:?}, ", wit_account.unwrap()));
                }
                if wit_balance.is_some() {
                    let wit_balance = wit_balance.unwrap();
                    metrics.push_str(&format!("\"witBalance\": {:.5}, ", wit_balance));
                    if let Some(start_wit_balance) = start_wit_balance {
                        let wit_hourly_expenditure =
                            ((start_wit_balance - wit_balance) / running_secs as f64) * 3600_f64;
                        metrics.push_str(&format!(
                            "\"witHourlyExpenditure\": {:.1}, ",
                            wit_hourly_expenditure
                        ));
                    }
                }
                metrics.push_str(&format!("\"witNodeSocket\": \"{wit_jsonrpc_socket}\", "));
                if wit_utxos_above_threshold.is_some() {
                    metrics.push_str(&format!(
                        "\"witUtxosAboveThreshold\": {}, ",
                        wit_utxos_above_threshold.unwrap()
                    ));
                }
            }

            let eth_balance = match check_eth_account_balance(&eth_jsonrpc_url, eth_account).await {
                Ok(eth_balance) => eth_balance,
                Err(err) => {
                    if status == WatchDogStatus::UpAndRunning {
                        status = err;
                    }
                    None
                }
            };

            if eth_balance.is_some() {
                let eth_balance = eth_balance.unwrap();
                metrics.push_str(&format!("\"evmBalance\": {:.5}, ", eth_balance));
                metrics.push_str(&format!("\"evmContract\": \"{eth_contract_address}\", "));
                if let Some(start_eth_balance) = start_eth_balance {
                    let eth_hourly_earnings =
                        ((eth_balance - start_eth_balance) / running_secs as f64) * 3600_f64;
                    metrics.push_str(&format!(
                        "\"evmHourlyEarnings\": {:.5}, ",
                        eth_hourly_earnings
                    ));
                }
            }
            
            metrics.push_str(&format!("\"runningSecs\": {running_secs}, "));
            metrics.push_str(&format!("\"status\": \"{}\"", status.to_string()));
            metrics.push_str("}}");
            log::info!("{metrics}");
            
            (eth_balance, wit_balance)
        };

        ctx.spawn(
            fut.into_actor(self)
                .then(move |(eth_balance, wit_balance), _act, ctx| {
                    // Schedule next iteration only when finished,
                    // as to avoid multiple tasks running in parallel
                    ctx.run_later(period, move |act, ctx| {
                        act.watch_global_status(eth_balance, wit_balance, ctx, period);
                    });
                    actix::fut::ready(())
                }),
        );
    }
}

async fn check_eth_account_balance(
    eth_jsonrpc_url: &str,
    eth_account: H160,
) -> Result<Option<f64>, WatchDogStatus> {
    let web3_http = web3::transports::Http::new(eth_jsonrpc_url)
        .map_err(|_e| WatchDogStatus::EvmDisconnect)
        .unwrap();

    let web3 = web3::Web3::new(web3_http);
    match web3.eth().syncing().await {
        Ok(syncing) => match syncing {
            web3::types::SyncState::NotSyncing => {
                match web3.eth().balance(eth_account, None).await {
                    Ok(eth_balance) => {
                        let eth_balance: f64 = eth_balance.to_string().parse().unwrap_or_default();
                        Ok(Some(eth_balance / 1000000000000000000.0))
                    }
                    _ => Ok(None),
                }
            }
            web3::types::SyncState::Syncing(_) => Err(WatchDogStatus::EvmSyncing),
        },
        Err(e) => {
            log::debug!("check_eth_account_balance => {}", e);
            
            Err(WatchDogStatus::EvmErrors)
        }
    }
}

async fn check_wit_connection_status(wit_client: &Addr<JsonRpcClient>) -> Result<(), WatchDogStatus> {
    let req = jsonrpc::Request::method("syncStatus").timeout(Duration::from_secs(5));
    let res = wit_client.send(req).await;
    match res {
        Ok(Ok(result)) => {
            if let Some(node_state) = result["node_state"].as_str() {
                match node_state {
                    "Synced" => Ok(()),
                    "AlmostSynced" => Err(WatchDogStatus::WitAlmostSynced),
                    "WaitingConsensus" => Err(WatchDogStatus::WitWaitingConsensus),
                    _ => Err(WatchDogStatus::WitSyncing)
                }
            } else {
                log::debug!("check_wit_connection_status => unknown node_state");
                Err(WatchDogStatus::WitErrors)
            }
        }
        Ok(Err(err)) => {
            log::debug!("check_wit_connection_status => {}", err);
            Err(WatchDogStatus::WitDisconnect)
        }
        Err(err) => {
            log::debug!("check_wit_connection_status => {}", err);
            Err(WatchDogStatus::WitDisconnect)
        }
    }
}

async fn fetch_wit_info (
    wit_client: &Addr<JsonRpcClient>,
    wit_utxos_min_threshold: u64,
) -> Result<(Option<String>, Option<f64>, Option<u64>), WatchDogStatus> {
    let req = jsonrpc::Request::method("getPkh").timeout(Duration::from_secs(5));
    let res = wit_client.send(req).await;
    let wit_account = match res {
        Ok(Ok(res)) => match serde_json::from_value::<String>(res) {
            Ok(pkh) => Some(pkh),
            Err(_) => None,
        },
        Ok(Err(_)) => None,
        Err(err) => {
            log::debug!("fetch_wit_info => {}", err);
            return Err(WatchDogStatus::WitErrors);
        }
    };

    let wit_account_balance = match wit_account.clone() {
        Some(wit_account) => {
            let req = jsonrpc::Request::method("getBalance")
                .timeout(Duration::from_secs(5))
                .params(wit_account)
                .expect("getBalance wrong params");
            let res = wit_client.send(req).await;
            let res = match res {
                Ok(res) => res,
                Err(err) => {
                    log::debug!("fetch_wit_info => {}", err);
                    return Err(WatchDogStatus::WitErrors);
                }
            };    
            match res {
                Ok(value) => match value.get("total") {
                    Some(value) => match value.as_f64() {
                        Some(value) => Some(value / 1000000000.0),
                        None => None,
                    },
                    None => None,
                },
                Err(err) => {
                    log::debug!("fetch_wit_info => {}", err);
                    return Err(WatchDogStatus::WitErrors);
                }
            }
        }
        None => None,
    };

    let wit_utxos_above_threshold = match wit_account.clone() {
        Some(wit_account) => {
            let req = jsonrpc::Request::method("getUtxoInfo")
                .timeout(Duration::from_secs(5))
                .params(wit_account)
                .expect("getUtxoInfo wrong params");
            let res = wit_client.send(req).await;
            let res = match res {
                Ok(res) => res,
                Err(err) => {
                    log::debug!("fetch_wit_info => {}", err);
                    return Err(WatchDogStatus::WitErrors);
                }
            };
            match res {
                Ok(utxo_info) => {
                    if let Some(utxos) = utxo_info["utxos"].as_array() {
                        let mut counter: u64 = u64::default();
                        for utxo in utxos {
                            if let Some(value) = utxo["value"].as_u64() {
                                if value >= wit_utxos_min_threshold {
                                    counter += 1;
                                }
                            }
                        }

                        Some(counter)
                    } else {
                        None
                    }
                }
                Err(err) => {
                    log::debug!("fetch_wit_info => {}", err);
                    return Err(WatchDogStatus::WitErrors);
                }
            }
        }
        None => None,
    };
    

    Ok((wit_account, wit_account_balance, wit_utxos_above_threshold))
}
