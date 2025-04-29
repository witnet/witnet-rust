use crate::{
    actors::dr_database::{
        DrDatabase, DrInfoBridge, DrState, GetLastDrId, SetDrInfoBridge, SetDrState,
        WitnetQueryStatus,
    },
    config::Config,
};
use actix::prelude::*;
use std::{convert::TryFrom, sync::Arc, time::Duration};
use web3::{
    contract::{self, Contract},
    ethabi::{Bytes, Token},
    transports::Http,
    types::U256,
    Web3,
};
use witnet_node::utils::stop_system_if_panicking;

/// EthPoller actor reads periodically new requests from the WRB Contract and includes them
/// in the DrDatabase
#[derive(Default)]
pub struct EthPoller {
    /// Web3 object
    pub web3: Option<Web3<web3::transports::Http>>,
    /// WRB contract
    pub wrb_contract: Option<Arc<Contract<web3::transports::Http>>>,
    /// Period to check for new requests in the WRB
    pub polling_rate_ms: u64,
    /// Skip first requests up to index n when updating database
    pub skip_first: u64,
    /// Max number of queries to be batched together
    pub max_batch_size: u16,
}

impl Drop for EthPoller {
    fn drop(&mut self) {
        log::trace!("Dropping EthPoller");
        stop_system_if_panicking("EthPoller");
    }
}

/// Make actor from EthPoller
impl Actor for EthPoller {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("EthPoller actor has been started!");

        self.check_new_requests_from_ethereum(ctx, Duration::from_millis(self.polling_rate_ms));
    }
}

/// Required trait for being able to retrieve EthPoller address from system registry
impl actix::Supervised for EthPoller {}

/// Required trait for being able to retrieve EthPoller address from system registry
impl SystemService for EthPoller {}

impl EthPoller {
    /// Initialize `PeersManager` taking the configuration from a `Config` structure
    pub fn from_config(
        config: &Config,
        web3: Web3<Http>,
        wrb_contract: Arc<Contract<Http>>,
    ) -> Self {
        Self {
            web3: Some(web3),
            wrb_contract: Some(wrb_contract),
            polling_rate_ms: config.eth_new_drs_polling_rate_ms,
            skip_first: config.storage_skip_first.unwrap_or(0),
            max_batch_size: config.eth_max_batch_size,
        }
    }

    fn check_new_requests_from_ethereum(&self, ctx: &mut Context<Self>, period: Duration) {
        let wrb_contract = self.wrb_contract.clone().unwrap();
        let skip_first = U256::from(self.skip_first);
        let max_batch_size = self.max_batch_size;

        log::debug!("Polling WitnetOracle at {:?}", wrb_contract.address());

        // Check requests
        let fut = async move {
            let dr_database_addr = DrDatabase::from_registry();

            let next_dr_id: Result<U256, web3::contract::Error> = wrb_contract
                .query(
                    "getNextQueryId",
                    (),
                    None,
                    contract::Options::default(),
                    None,
                )
                .await
                .inspect_err(|err| {
                    log::error!(
                        "Fail to read getNextQueryId from contract: {:?}",
                        err.to_string()
                    )
                });

            let last_dr_id = dr_database_addr.send(GetLastDrId).await;

            if let (Ok(next_dr_id), Ok(Ok(mut last_dr_id))) = (next_dr_id, last_dr_id) {
                if last_dr_id < skip_first {
                    log::debug!(
                        "Skipping first {} queries as per SKIP_FIRST config param",
                        skip_first
                    );
                    last_dr_id = skip_first;
                }
                while last_dr_id + 1 < next_dr_id {
                    let init_index = usize::try_from(last_dr_id + 1).unwrap();
                    let last_index = match next_dr_id.cmp(&(last_dr_id + max_batch_size)) {
                        std::cmp::Ordering::Greater => {
                            usize::try_from(last_dr_id + max_batch_size).unwrap()
                        }
                        _ => usize::try_from(next_dr_id).unwrap(),
                    };
                    let ids = init_index..last_index;
                    let ids: Vec<Token> = ids.map(|id| Token::Uint(id.into())).collect();

                    last_dr_id += U256::from(max_batch_size);

                    let queries_status: Result<Vec<Token>, web3::contract::Error> = wrb_contract
                        .query(
                            "getQueryStatusBatch",
                            ids.clone(),
                            None,
                            contract::Options::default(),
                            None,
                        )
                        .await;

                    if let Ok(queries_status) = queries_status {
                        let mut posted_ids: Vec<Token> = vec![];
                        for (pos, status) in queries_status.iter().enumerate() {
                            let query_id = ids[pos].to_owned().into_uint().unwrap();
                            let status: u8 = status.to_string().parse().unwrap();
                            match WitnetQueryStatus::from_code(status) {
                                WitnetQueryStatus::Unknown => {
                                    log::warn!("Skipped unavailable query [{}]", query_id);
                                    dr_database_addr.do_send(SetDrState {
                                        dr_id: query_id,
                                        dr_state: DrState::Dismissed,
                                    });
                                }
                                WitnetQueryStatus::Posted => {
                                    log::info!("Detected new query [{}]", query_id);
                                    posted_ids.push(Token::Uint(query_id));
                                }
                                WitnetQueryStatus::Reported | WitnetQueryStatus::Finalized => {
                                    log::info!("Skipped already solved query [{}]", query_id);
                                    dr_database_addr.do_send(SetDrState {
                                        dr_id: query_id,
                                        dr_state: DrState::Finished,
                                    });
                                }
                            }
                        }
                        if !posted_ids.is_empty() {
                            let dr_bytecodes: Result<Vec<Bytes>, web3::contract::Error> =
                                wrb_contract
                                    .query(
                                        "extractWitnetDataRequests",
                                        posted_ids.clone(),
                                        None,
                                        contract::Options::default(),
                                        None,
                                    )
                                    .await;
                            if let Ok(dr_bytecodes) = dr_bytecodes {
                                for (pos, dr_id) in posted_ids.iter().enumerate() {
                                    dr_database_addr.do_send(SetDrInfoBridge(
                                        dr_id.to_owned().into_uint().unwrap(),
                                        DrInfoBridge {
                                            dr_bytes: dr_bytecodes[pos].to_owned(),
                                            ..Default::default()
                                        },
                                    ));
                                }
                            } else {
                                log::error!(
                                    "Fail to extract Witnet request bytecodes from queries {:?}",
                                    posted_ids
                                );
                            }
                        }
                    } else {
                        log::error!(
                            "Fail to get status of queries #{} to #{}: {}",
                            init_index,
                            next_dr_id,
                            queries_status.unwrap_err()
                        );
                    }
                }
            }
        };

        ctx.spawn(fut.into_actor(self).then(move |(), _act, ctx| {
            // Wait until the function finished to schedule next call.
            // This avoids tasks running in parallel.
            ctx.run_later(period, move |act, ctx| {
                // Reschedule check_new_requests_from_ethereum
                act.check_new_requests_from_ethereum(ctx, period);
            });

            actix::fut::ready(())
        }));
    }
}
