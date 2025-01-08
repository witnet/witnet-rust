use crate::{
    actors::dr_database::{CountDrsPerState, DrDatabase},
    config::Config,
};
use actix::prelude::*;
use chrono::{NaiveTime, Timelike, Utc};
use core::fmt;
use std::{
    convert::TryFrom,
    sync::Arc,
    time::{Duration, Instant},
};
use web3::{
    contract::{self, Contract},
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
    pub polling_rate_minutes: u32,
    /// Instant at which the actor is created
    pub start_ts: Option<Instant>,
    /// Eth balance upon first metric report:
    pub start_eth_balance: Option<f64>,
    /// Wit balance upon last refund
    pub start_wit_balance: Option<f64>,
    /// Past data request cumulative counters:
    pub drs_history: Option<(u32, u32, u32)>,
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

        self.watch_global_status(None, None, None, ctx);
    }
}

#[derive(Debug, PartialEq)]
enum WatchDogStatus {
    EvmBalanceLeak,
    EvmDisconnect,
    EvmErrors,
    EvmSyncing,
    WitAlmostSynced,
    WitBalanceLow,
    WitErrors,
    WitDisconnect,
    WitSyncing,
    WitUtxosLow,
    WitWaitingConsensus,
    UpAndRestarted,
    UpAndRunning,
}

impl fmt::Display for WatchDogStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WatchDogStatus::EvmBalanceLeak => write!(f, "evm-balance-leak"),
            WatchDogStatus::EvmDisconnect => write!(f, "evm-disconnect"),
            WatchDogStatus::EvmErrors => write!(f, "evm-errors"),
            WatchDogStatus::EvmSyncing => write!(f, "evm-syncing"),
            WatchDogStatus::WitAlmostSynced => write!(f, "wit-almost-synced"),
            WatchDogStatus::WitBalanceLow => write!(f, "wit-balance-low"),
            WatchDogStatus::WitDisconnect => write!(f, "wit-disconnect"),
            WatchDogStatus::WitErrors => write!(f, "wit-errors"),
            WatchDogStatus::WitSyncing => write!(f, "wit-syncing"),
            WatchDogStatus::WitUtxosLow => write!(f, "wit-utxos-low"),
            WatchDogStatus::WitWaitingConsensus => write!(f, "wit-waiting-consensus"),
            WatchDogStatus::UpAndRestarted => write!(f, "up-and-restarted"),
            WatchDogStatus::UpAndRunning => write!(f, "up-and-running"),
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
            wit_client: JsonRpcClient::start(config.witnet_jsonrpc_socket.to_string().as_str())
                .ok(),
            wit_jsonrpc_socket: config.witnet_jsonrpc_socket.to_string(),
            wit_utxo_min_value_threshold: config.witnet_utxo_min_value_threshold,
            eth_account: config.eth_from,
            eth_contract: Some(eth_contract),
            eth_jsonrpc_url: config.eth_jsonrpc_url.clone(),
            polling_rate_minutes: config.watch_dog_polling_rate_minutes,
            start_ts: Some(Instant::now()),
            start_eth_balance: None,
            start_wit_balance: None,
            drs_history: None,
        }
    }

    fn watch_global_status(
        &mut self,
        eth_balance: Option<f64>,
        wit_balance: Option<f64>,
        drs_history: Option<(u32, u32, u32)>,
        ctx: &mut Context<Self>,
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
        if self.drs_history.is_none() && drs_history.is_some() {
            self.drs_history = drs_history;
        }
        let start_eth_balance = self.start_eth_balance;
        let start_wit_balance = self.start_wit_balance;
        let wit_client = self.wit_client.clone();
        let wit_jsonrpc_socket = self.wit_jsonrpc_socket.clone();
        let mut wit_next_balance = wit_balance;
        let wit_utxo_min_value_threshold = self.wit_utxo_min_value_threshold;
        let eth_jsonrpc_url = self.eth_jsonrpc_url.clone();
        let eth_account = self.eth_account;
        let eth_contract = self.eth_contract.clone().unwrap();
        let eth_contract_address = eth_contract.address();
        let running_secs =
            u32::try_from(self.start_ts.unwrap().elapsed().as_secs()).unwrap_or_default();
        let mut drs_history = drs_history.unwrap_or_default();

        let fut = async move {
            let mut status = WatchDogStatus::UpAndRunning;

            let dr_database = DrDatabase::from_registry();
            let (drs_new, drs_pending, drs_finished, drs_dismissed) =
                dr_database.send(CountDrsPerState).await.unwrap().unwrap();
            let total_queries = drs_new + drs_pending + drs_finished + drs_dismissed;

            let mut metrics: String = "{".to_string();

            metrics.push_str(&format!(
                "\"drsCurrentlyPending\": {}, ",
                drs_new + drs_pending
            ));

            drs_history = if drs_history != (0u32, 0u32, 0u32) {
                let daily_queries = (f64::from(total_queries - drs_history.2)
                    / f64::from(running_secs))
                    * 86400_f64;
                metrics.push_str(&format!("\"drsDailyQueries\": {:.1}, ", daily_queries));

                let last_dismissed = drs_dismissed - drs_history.1;
                metrics.push_str(&format!("\"drsLastDismissed\": {last_dismissed}, "));

                let last_reported = drs_finished - drs_history.0;
                metrics.push_str(&format!("\"drsLastReported\": {last_reported}, "));

                // preserve the number of total queries counted upon bridge launch,
                // so average queries per day can be averaged:
                (drs_finished, drs_dismissed, drs_history.2)
            } else {
                status = WatchDogStatus::UpAndRestarted;
                (drs_finished, drs_dismissed, total_queries)
            };
            metrics.push_str(&format!("\"drsTotalQueries\": {total_queries}, "));

            let eth_balance = match (
                eth_balance,
                check_eth_account_balance(&eth_jsonrpc_url, eth_account).await,
            ) {
                (Some(eth_balance), Ok(Some(new_balance))) => {
                    if status == WatchDogStatus::UpAndRunning && new_balance < eth_balance {
                        status = WatchDogStatus::EvmBalanceLeak
                    }
                    Some(new_balance)
                }
                (_, Ok(new_balance)) => new_balance,
                (_, Err(err)) => {
                    if status == WatchDogStatus::UpAndRunning {
                        status = err;
                    }
                    None
                }
            };

            let eth_contract_class: Option<String> = match eth_contract
                .query("class", (), None, contract::Options::default(), None)
                .await
            {
                Ok(version) => Some(version),
                Err(err) => {
                    log::error!(
                        "Fail to read class() from contract at {:?}: {:?}",
                        eth_contract_address,
                        err.to_string()
                    );
                    if status == WatchDogStatus::UpAndRunning {
                        status = WatchDogStatus::EvmErrors;
                    }
                    None
                }
            };

            let eth_contract_version: Option<String> = match eth_contract
                .query("version", (), None, contract::Options::default(), None)
                .await
            {
                Ok(version) => Some(version),
                Err(web3::contract::Error::InterfaceUnsupported) => None,
                Err(err) => {
                    log::error!(
                        "Fail to read version() from contract at {:?}: {:?}",
                        eth_contract_address,
                        err.to_string()
                    );
                    if status == WatchDogStatus::UpAndRunning {
                        status = WatchDogStatus::EvmErrors;
                    }
                    None
                }
            };

            metrics.push_str(&format!("\"evmAccount\": \"{eth_account}\", "));
            if eth_balance.is_some() {
                let eth_balance = eth_balance.unwrap();
                metrics.push_str(&format!("\"evmBalance\": {:.5}, ", eth_balance));
                metrics.push_str(&format!("\"evmContract\": \"{eth_contract_address}\", "));
                if let Some(eth_contract_class) = eth_contract_class {
                    if let Some(eth_contract_version) = eth_contract_version {
                        metrics.push_str(&format!(
                            "\"evmContractVersion\": \"{}:{}\", ",
                            eth_contract_class, eth_contract_version
                        ));
                    } else {
                        metrics.push_str(&format!(
                            "\"evmContractVersion\": {:?}, ",
                            eth_contract_class
                        ));
                    }
                }
                if let Some(start_eth_balance) = start_eth_balance {
                    let eth_hourly_earnings =
                        ((eth_balance - start_eth_balance) / f64::from(running_secs)) * 3600_f64;
                    metrics.push_str(&format!(
                        "\"evmHourlyEarnings\": {:.5}, ",
                        eth_hourly_earnings
                    ));
                }
            }

            if let Some(wit_client) = wit_client {
                if let Err(err) = check_wit_connection_status(&wit_client).await {
                    status = err;
                }

                let (wit_account, wit_balance, wit_utxos_above_threshold) =
                    match fetch_wit_info(&wit_client, wit_utxo_min_value_threshold).await {
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
                        let wit_hourly_expenditure = ((start_wit_balance - wit_balance)
                            / f64::from(running_secs))
                            * 3600_f64;
                        metrics.push_str(&format!(
                            "\"witHourlyExpenditure\": {:.1}, ",
                            wit_hourly_expenditure
                        ));
                        if wit_hourly_expenditure > 0.0
                            && wit_balance / wit_hourly_expenditure < 72.0
                            && status == WatchDogStatus::UpAndRunning
                        {
                            status = WatchDogStatus::WitBalanceLow;
                        }
                    }
                }
                metrics.push_str(&format!("\"witNodeSocket\": \"{wit_jsonrpc_socket}\", "));
                if let Some(wit_utxos_above_threshold) = wit_utxos_above_threshold {
                    metrics.push_str(&format!(
                        "\"witUtxosAboveThreshold\": {}, ",
                        wit_utxos_above_threshold
                    ));
                    if wit_utxos_above_threshold < 10 && status == WatchDogStatus::UpAndRunning {
                        status = WatchDogStatus::WitUtxosLow;
                    }
                }

                wit_next_balance = wit_balance;
            }

            metrics.push_str(&format!("\"runningSecs\": {running_secs}, "));
            metrics.push_str(&format!("\"status\": \"{}\"", status));
            metrics.push('}');

            log::info!("{metrics}");

            (eth_balance, wit_next_balance, Some(drs_history))
        };

        ctx.spawn(fut.into_actor(self).then(
            move |(eth_balance, wit_balance, drs_history), act, ctx| {
                let time_now = Utc::now().time();
                let period_minutes = act.polling_rate_minutes;
                let time_next_minute =
                    period_minutes * (time_now.minute().div_euclid(period_minutes) + 1);
                let time_next = if time_next_minute >= 60 {
                    NaiveTime::from_hms_opt(time_now.hour() + 1, time_next_minute - 60, 0)
                } else {
                    NaiveTime::from_hms_opt(time_now.hour(), time_next_minute, 0)
                };
                let dur = if let Some(time_next) = time_next {
                    let num_nanosecs = (time_next - time_now).num_nanoseconds();
                    if let Some(num_nanosecs) = num_nanosecs {
                        Duration::from_nanos(num_nanosecs.unsigned_abs())
                    } else {
                        Duration::from_secs(u64::from(period_minutes * 60))
                    }
                } else {
                    Duration::from_secs(u64::from(period_minutes * 60))
                };
                // Schedule next iteration only when finished,
                // as to avoid multiple tasks running in parallel
                ctx.run_later(dur, move |act, ctx| {
                    act.watch_global_status(eth_balance, wit_balance, drs_history, ctx);
                });
                actix::fut::ready(())
            },
        ));
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

async fn check_wit_connection_status(
    wit_client: &Addr<JsonRpcClient>,
) -> Result<(), WatchDogStatus> {
    let req = jsonrpc::Request::method("syncStatus").timeout(Duration::from_secs(5));
    let res = wit_client.send(req).await;
    match res {
        Ok(Ok(result)) => {
            if let Some(node_state) = result["node_state"].as_str() {
                match node_state {
                    "Synced" => Ok(()),
                    "AlmostSynced" => Err(WatchDogStatus::WitAlmostSynced),
                    "WaitingConsensus" => Err(WatchDogStatus::WitWaitingConsensus),
                    _ => Err(WatchDogStatus::WitSyncing),
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

async fn fetch_wit_info(
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
            log::debug!("fetch_wit_info => method: getPkh; error: {}", err);
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
                    log::debug!("fetch_wit_info => method: getBalance; error: {}", err);
                    return Err(WatchDogStatus::WitErrors);
                }
            };
            match res {
                Ok(value) => match value.get("total") {
                    Some(value) => value.as_f64().map(|value| value / 1000000000.0),
                    None => None,
                },
                Err(err) => {
                    log::debug!("fetch_wit_info => method: getBalance; error: {}", err);
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
                    log::debug!("fetch_wit_info => method: getUtxoInfo; error: {}", err);
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
                    log::debug!("fetch_wit_info => method: getUtxoInfo; error: {}", err);
                    return Err(WatchDogStatus::WitErrors);
                }
            }
        }
        None => None,
    };

    Ok((wit_account, wit_account_balance, wit_utxos_above_threshold))
}
