use crate::{
    actors::dr_database::{
        DrDatabase, DrInfoBridge, GetLastDrId, SetDrInfoBridge, WitnetQueryStatus,
    },
    config::Config,
};
use actix::prelude::*;
use std::{convert::TryFrom, sync::Arc, time::Duration};
use web3::{
    contract::{self, Contract},
    ethabi::Bytes,
    transports::Http,
    types::U256,
};
use witnet_node::utils::stop_system_if_panicking;

/// EthPoller actor reads periodically new requests from the WRB Contract and includes them
/// in the DrDatabase
#[derive(Default)]
pub struct EthPoller {
    /// WRB contract
    pub wrb_contract: Option<Arc<Contract<web3::transports::Http>>>,
    /// Period to check for new requests in the WRB
    pub eth_new_dr_polling_rate_ms: u64,
    /// Skip first requests up to index n when updating database
    pub skip_first: u64,
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

        self.check_new_requests_from_ethereum(
            ctx,
            Duration::from_millis(self.eth_new_dr_polling_rate_ms),
        );
    }
}

/// Required trait for being able to retrieve EthPoller address from system registry
impl actix::Supervised for EthPoller {}

/// Required trait for being able to retrieve EthPoller address from system registry
impl SystemService for EthPoller {}

impl EthPoller {
    /// Initialize `PeersManager` taking the configuration from a `Config` structure
    pub fn from_config(config: &Config, wrb_contract: Arc<Contract<Http>>) -> Self {
        Self {
            wrb_contract: Some(wrb_contract),
            eth_new_dr_polling_rate_ms: config.eth_new_dr_polling_rate_ms,
            skip_first: config.skip_first.unwrap_or(0),
        }
    }

    fn check_new_requests_from_ethereum(&self, ctx: &mut Context<Self>, period: Duration) {
        log::debug!("Checking new DRs from Ethereum contract...");

        let wrb_contract = self.wrb_contract.clone().unwrap();
        let skip_first = U256::from(self.skip_first);
        // Check requests
        let fut = async move {
            let total_requests_count: Result<U256, web3::contract::Error> = wrb_contract
                .query(
                    "getNextQueryId",
                    (),
                    None,
                    contract::Options::default(),
                    None,
                )
                .await
                .map_err(|err| {
                    log::error!(
                        "Fail to read getNextQueryId from contract: {:?}",
                        err.to_string()
                    );

                    err
                });

            let dr_database_addr = DrDatabase::from_registry();
            let db_request_count = dr_database_addr.send(GetLastDrId).await;

            if let (Ok(total_requests_count), Ok(Ok(mut db_request_count))) =
                (total_requests_count, db_request_count)
            {
                if db_request_count < skip_first {
                    log::debug!(
                        "Skipping first {} requests per skip_first config param",
                        skip_first
                    );
                    db_request_count = skip_first;
                }
                if db_request_count < total_requests_count {
                    let init_index = usize::try_from(db_request_count + 1).unwrap();
                    let last_index = usize::try_from(total_requests_count).unwrap();

                    for i in init_index..last_index {
                        log::debug!("[{}] checking dr in wrb", i);

                        let query_status: Result<u8, web3::contract::Error> = wrb_contract
                            .query(
                                "getQueryStatus",
                                (U256::from(i),),
                                None,
                                contract::Options::default(),
                                None,
                            )
                            .await;

                        match query_status {
                            Ok(status) => match WitnetQueryStatus::from_code(status) {
                                WitnetQueryStatus::Unknown => {
                                    log::debug!("[{}] has not exist, skipping", i)
                                }
                                WitnetQueryStatus::Posted => {
                                    log::info!("[{}] new dr in wrb", i);
                                    if let Ok(set_dr_info_bridge) =
                                        process_posted_request(i.into(), &wrb_contract).await
                                    {
                                        dr_database_addr.do_send(set_dr_info_bridge);
                                    } else {
                                        break;
                                    }
                                }
                                WitnetQueryStatus::Reported => {
                                    log::debug!("[{}] already reported", i);
                                    if let Ok(set_dr_info_bridge) =
                                        process_posted_request(i.into(), &wrb_contract).await
                                    {
                                        dr_database_addr.do_send(set_dr_info_bridge);
                                    } else {
                                        break;
                                    }
                                }
                                WitnetQueryStatus::Deleted => {
                                    log::debug!("[{}] has been deleted, skipping", i)
                                }
                            },
                            Err(err) => {
                                log::error!(
                                    "Fail to read getQueryStatus from contract: {:?}",
                                    err.to_string(),
                                );
                                break;
                            }
                        }
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

/// Auxiliary function that process the information of a new posted request
async fn process_posted_request(
    query_id: U256,
    wrb_contract: &Contract<web3::transports::Http>,
) -> Result<SetDrInfoBridge, web3::contract::Error> {
    let dr_bytes: Result<Bytes, web3::contract::Error> = wrb_contract
        .query(
            "readRequestBytecode",
            (query_id,),
            None,
            contract::Options::default(),
            None,
        )
        .await;

    // Re-route some errors as success (explanation below)
    match dr_bytes {
        Ok(dr_bytes) => Ok(dr_bytes),
        Err(err) => {
            log::error!("Fail to read dr bytes from contract: {}", err.to_string());

            // In some versions of the bridge contracts (those based on
            // `WitnetRequestBoardTrustableBase`), we may get a revert when trying to fetch the dr
            // bytes for a deleted query.
            // If that's the case, we can return a success here, with empty bytes, so that the
            // request can locally marked as complete, and we can move on.
            if err.to_string().contains("WitnetRequestBoardTrustableBase") {
                log::error!("Wait! This is an instance of `WitnetRequestBoardTrustableBase`. Let's assume we got a revert because the dr bytes were deleted, and simply move on.");

                Ok(Default::default())
            // Otherwise, handle the error normally
            } else {
                Err(err)
            }
        }
    // Wrap the dr bytes in a `SetDrInfoBridge` structure
    }.map(|dr_bytes| SetDrInfoBridge(
        query_id,
        DrInfoBridge {
            dr_bytes,
            ..Default::default()
        },
    ))
}
