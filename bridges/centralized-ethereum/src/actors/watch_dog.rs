use crate::{
    actors::dr_database::{CountDrsPerState, DrDatabase},
    config::Config,
};
use actix::prelude::*;
use async_jsonrpc_client::{transports::tcp::TcpSocket, Transport};
use futures_util::compat::Compat01As03;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use web3::{
    contract::Contract,
    transports::Http,
    types::{H160, U256},
};
use witnet_net::client::tcp::{jsonrpc, JsonRpcClient};
use witnet_node::utils::stop_system_if_panicking;

/// EthPoller actor reads periodically new requests from the WRB Contract and includes them
/// in the DrDatabase
#[derive(Default)]
pub struct WatchDog {
    /// JSON RPC connection to Wit/node
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

#[derive(Serialize, Deserialize)]
struct WatchDogOutput {
    pub running_secs: u64,
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

/// Required trait for being able to retrieve WatchDog address from system registry
impl actix::Supervised for WatchDog {}
impl SystemService for WatchDog {}

impl WatchDog {
    /// Initialize from config
    pub fn from_config(config: &Config, eth_contract: Arc<Contract<Http>>) -> Self {
        Self {
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
        let wit_jsonrpc_socket = self.wit_jsonrpc_socket.clone();
        let wit_utxo_min_value_threshold = self.wit_utxo_min_value_threshold;
        let eth_jsonrpc_url = self.eth_jsonrpc_url.clone();
        let eth_account = self.eth_account;
        let eth_contract_address = self.eth_contract.clone().unwrap().address();
        let running_secs = self.start_ts.unwrap().elapsed().as_secs();

        let fut = async move {
            let mut status = "up-and-running".to_string();

            if let Err(err) = check_wit_connection_status(&wit_jsonrpc_socket).await {
                status = err;
            }
            let wit_client = JsonRpcClient::start(&wit_jsonrpc_socket)
                .expect("cannot start JSON/WIT connection");
            let wit_account = match fetch_wit_account(&wit_client).await {
                Ok(pkh) => pkh,
                Err(err) => {
                    if status.eq("up-and-running") {
                        status = err;
                    }
                    None
                }
            };

            let wit_balance = match wit_account.clone() {
                Some(pkh) => match fetch_wit_account_balance(&wit_client, pkh.as_str()).await {
                    Ok(wit_balance) => wit_balance,
                    Err(err) => {
                        if status.eq("up-and-running") {
                            status = err;
                        }
                        None
                    }
                },
                None => None,
            };

            let wit_utxos_above_threshold = match wit_account.clone() {
                Some(pkh) => {
                    match fetch_wit_account_count_utxos_above(
                        &wit_client,
                        pkh.as_str(),
                        wit_utxo_min_value_threshold,
                    )
                    .await
                    {
                        Ok(wit_utxos_above_threshold) => wit_utxos_above_threshold,
                        Err(err) => {
                            if status.eq("up-and-running") {
                                status = err;
                            }
                            None
                        }
                    }
                }
                None => None,
            };

            let eth_balance = match check_eth_account_balance(&eth_jsonrpc_url, eth_account).await {
                Ok(Some(eth_balance)) => {
                    let eth_balance: f64 = eth_balance.to_string().parse().unwrap_or_default();
                    //Some(Unit::Wei(&eth_balance.to_string()).to_eth_str().unwrap_or_default()),
                    Some(eth_balance / 1000000000000000000.0)
                }
                Ok(None) => None,
                Err(err) => {
                    if status.eq("up-and-running") {
                        status = err;
                    }
                    None
                }
            };

            let dr_database = DrDatabase::from_registry();
            let (_, drs_pending, drs_finished, _) =
                dr_database.send(CountDrsPerState).await.unwrap().unwrap();

            let mut metrics: String = "{".to_string();
            metrics.push_str(&format!("\"drsFinished\": {drs_finished}, "));
            metrics.push_str(&format!("\"drsPending\": {drs_pending}, "));
            metrics.push_str(&format!("\"evmAccount\": \"{eth_account}\", "));
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
            metrics.push_str(&format!("\"witNodeSocket\": \"{}\", ", wit_jsonrpc_socket));
            if wit_utxos_above_threshold.is_some() {
                metrics.push_str(&format!(
                    "\"witUtxosAboveThreshold\": {}, ",
                    wit_utxos_above_threshold.unwrap()
                ));
            }
            metrics.push_str(&format!("\"runningSecs\": {running_secs}, "));
            metrics.push_str(&format!("\"status\": \"{status}\""));
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
) -> Result<Option<U256>, String> {
    let web3_http = web3::transports::Http::new(eth_jsonrpc_url)
        .map_err(|_e| "evm-disconnect".to_string())
        .unwrap();

    let web3 = web3::Web3::new(web3_http);
    match web3.eth().syncing().await {
        Ok(syncing) => match syncing {
            web3::types::SyncState::NotSyncing => {
                match web3.eth().balance(eth_account, None).await {
                    Ok(balance) => Ok(Some(balance)),
                    _ => Ok(None),
                }
            }
            web3::types::SyncState::Syncing(_) => Err("evm-syncing".to_string()),
        },
        Err(_e) => Err("evm-errors".to_string()),
    }
}

async fn check_wit_connection_status(wit_jsonrpc_socket: &str) -> Result<(), String> {
    let (_handle, wit_client) = TcpSocket::new(wit_jsonrpc_socket).unwrap();
    let wit_client = Arc::new(wit_client);
    let res = wit_client.execute("syncStatus", json!(null));
    let res = Compat01As03::new(res);
    let res = tokio::time::timeout(Duration::from_secs(5), res).await;

    match res {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(_)) => Err("wit-syncing".to_string()),
        Err(_elapse) => Err("wit-disconnect".to_string()),
    }
}

async fn fetch_wit_account(wit_client: &Addr<JsonRpcClient>) -> Result<Option<String>, String> {
    let req = jsonrpc::Request::method("getPkh").timeout(Duration::from_secs(5));
    let res = wit_client.send(req).await;
    match res {
        Ok(Ok(res)) => match serde_json::from_value::<String>(res) {
            Ok(pkh) => Ok(Some(pkh)),
            Err(_) => Ok(None),
        },
        Ok(Err(_)) => Ok(None),
        Err(_) => Err("wit-errors-getPkh".to_string()),
    }
}

async fn fetch_wit_account_balance(
    wit_client: &Addr<JsonRpcClient>,
    wit_account: &str,
) -> Result<Option<f64>, String> {
    let req = jsonrpc::Request::method("getBalance")
        .timeout(Duration::from_secs(5))
        .params(vec![wit_account, "true"])
        .expect("getBalance wrong params");

    let res = wit_client.send(req).await;
    let res = match res {
        Ok(res) => res,
        Err(_) => {
            return Err("wit-errors-getBalance".to_string());
        }
    };

    match res {
        Ok(value) => match value.get("total") {
            Some(value) => match value.as_f64() {
                Some(value) => Ok(Some(value / 1000000000.0)),
                None => Ok(None),
            },
            None => Ok(None),
        },
        Err(_) => Err("wit-errors-getBalance".to_string()),
    }
}

async fn fetch_wit_account_count_utxos_above(
    wit_client: &Addr<JsonRpcClient>,
    wit_account: &str,
    threshold: u64,
) -> Result<Option<u64>, String> {
    let req = jsonrpc::Request::method("getUtxoInfo")
        .timeout(Duration::from_secs(5))
        .params(wit_account)
        .expect("getUtxoInfo wrong params");

    let res = wit_client.send(req).await;
    let res = match res {
        Ok(res) => res,
        Err(_) => {
            return Err("wit-errors-getUtxoInfo".to_string());
        }
    };

    match res {
        Ok(utxo_info) => {
            if let Some(utxos) = utxo_info["utxos"].as_array() {
                let mut counter: u64 = u64::default();
                for utxo in utxos {
                    if let Some(value) = utxo["value"].as_u64() {
                        if value >= threshold {
                            counter += 1;
                        }
                    }
                }

                Ok(Some(counter))
            } else {
                Ok(None)
            }
        }
        Err(_) => Err("wit-errors-getUtxoInfo".to_string()),
    }
}
