use std::{convert::TryFrom, time::Duration};

use actix::prelude::*;
use serde_json::json;

use witnet_data_structures::{
    chain::{Block, ConsensusConstants, DataRequestInfo, Hash, Hashable},
    proto::versioning::ProtocolInfo,
};
use witnet_net::client::tcp::{jsonrpc, JsonRpcClient};
use witnet_node::utils::stop_system_if_panicking;
use witnet_util::timestamp::get_timestamp;

use crate::{
    actors::{
        dr_database::{DrDatabase, DrInfoBridge, DrState, GetAllPendingDrs, SetDrInfoBridge},
        dr_reporter::{DrReporter, DrReporterMsg, Report},
    },
    config::Config,
};

/// WitPoller actor checks periodically the state of the requests in Witnet to call DrReporter
/// in case of found a tally
#[derive(Default)]
pub struct WitPoller {
    witnet_client: Option<Addr<JsonRpcClient>>,
    witnet_consensus_constants: Option<ConsensusConstants>,
    witnet_dr_txs_polling_rate_ms: u64,
    witnet_dr_txs_timeout_ms: u64,
}

impl Drop for WitPoller {
    fn drop(&mut self) {
        log::trace!("Dropping WitPoller");
        stop_system_if_panicking("WitPoller");
    }
}

/// Make actor from WitPoller
impl Actor for WitPoller {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("WitPoller actor has been started!");

        self.check_tally_pending_drs(
            ctx,
            Duration::from_millis(self.witnet_dr_txs_polling_rate_ms),
        )
    }
}

/// Required trait for being able to retrieve WitPoller address from system registry
impl actix::Supervised for WitPoller {}

/// Required trait for being able to retrieve WitPoller address from system registry
impl SystemService for WitPoller {}

impl WitPoller {
    /// Initialize the `WitPoller` taking the configuration from a `Config` structure
    /// and a Json-RPC client connected to a Witnet node
    pub fn from_config(config: &Config, node_client: Addr<JsonRpcClient>) -> Self {
        Self {
            witnet_client: Some(node_client),
            witnet_consensus_constants: None,
            witnet_dr_txs_polling_rate_ms: config.witnet_dr_txs_polling_rate_ms,
            witnet_dr_txs_timeout_ms: config.witnet_dr_txs_timeout_ms,
        }
    }

    fn check_tally_pending_drs(&self, ctx: &mut Context<Self>, period: Duration) {
        let witnet_client = self.witnet_client.clone().unwrap();
        let witnet_consensus_constants = self.witnet_consensus_constants.clone();
        let timeout_secs = i64::try_from(self.witnet_dr_txs_timeout_ms / 1000).unwrap();

        let fut = async move {
            let witnet_consensus_constants = match witnet_consensus_constants {
                Some(consensus_constants) => consensus_constants,
                None => match get_consensus_constants(witnet_client.clone()).await {
                    Ok(consensus_constants) => {
                        log::info!("Protocol consensus constants: {:?}", consensus_constants);
                        consensus_constants
                    }
                    Err(_) => {
                        return None;
                    }
                },
            };

            let dr_database_addr = DrDatabase::from_registry();
            let dr_reporter_addr = DrReporter::from_registry();
            let pending_drs = dr_database_addr
                .send(GetAllPendingDrs)
                .await
                .unwrap()
                .unwrap();
            let current_timestamp = get_timestamp();
            let mut dr_reporter_msgs = vec![];

            if !pending_drs.is_empty() {
                let witnet_protocol_info = match get_protocol_info(witnet_client.clone()).await {
                    Ok(x) => x,
                    Err(()) => {
                        log::error!("Failed to get current protocol info from witnet client, will retry later");
                        return Some(witnet_consensus_constants);
                    }
                };
                for (dr_id, dr_bytes, dr_tx_hash, dr_tx_creation_timestamp) in pending_drs {
                    let method = String::from("dataRequestReport");
                    let params = json!([dr_tx_hash]);
                    let req = jsonrpc::Request::method(method)
                        .timeout(Duration::from_millis(5_000))
                        .params(params)
                        .expect("params failed serialization");
                    let report = witnet_client.send(req).await;
                    let report = match report {
                        Ok(report) => report,
                        Err(_) => {
                            log::error!("Failed to connect to witnet client, will retry later");
                            break;
                        }
                    };

                    if let Ok(report) = report {
                        match serde_json::from_value::<Option<DataRequestInfo>>(report) {
                            Ok(Some(DataRequestInfo {
                                tally: Some(tally),
                                block_hash_dr_tx: Some(dr_block_hash),
                                current_commit_round: dr_commits_round,
                                ..
                            })) => {
                                log::info!("[{}] <= dr_tx = {}", dr_id, dr_tx_hash);

                                let result = tally.tally.clone();
                                // Get timestamp of the epoch at which all data request commit txs
                                // were incuded in the Witnet blockchain:
                                let dr_timestamp = match get_dr_timestamp(
                                    witnet_client.clone(),
                                    &witnet_consensus_constants,
                                    &witnet_protocol_info,
                                    dr_block_hash,
                                    dr_commits_round,
                                )
                                .await
                                {
                                    Ok(timestamp) => timestamp,
                                    Err(()) => continue,
                                };

                                dr_reporter_msgs.push(Report {
                                    dr_id,
                                    dr_timestamp,
                                    dr_tx_hash,
                                    dr_tally_tx_hash: tally.hash(),
                                    result,
                                });
                            }
                            Ok(..) => {
                                // the data request is being resolved, just not yet
                            }
                            Err(e) => {
                                log::error!(
                                    "[{}] => cannot deserialize dr_tx = {}: {:?}",
                                    dr_id,
                                    dr_tx_hash,
                                    e
                                );
                            }
                        };
                    } else {
                        log::debug!("[{}] <> dr_tx = {}", dr_id, dr_tx_hash);
                    }

                    let elapsed_secs = current_timestamp - dr_tx_creation_timestamp;
                    if elapsed_secs >= timeout_secs {
                        log::warn!(
                            "[{}] => will retry new dr_tx after {} secs",
                            dr_id,
                            elapsed_secs
                        );
                        DrDatabase::from_registry()
                            .send(SetDrInfoBridge(
                                dr_id,
                                DrInfoBridge {
                                    dr_bytes,
                                    dr_state: DrState::New,
                                    dr_tx_hash: None,
                                    dr_tx_creation_timestamp: None,
                                },
                            ))
                            .await
                            .unwrap();
                    }
                }
            }

            dr_reporter_addr
                .send(DrReporterMsg {
                    reports: dr_reporter_msgs,
                })
                .await
                .unwrap();

            Some(witnet_consensus_constants)
        };

        ctx.spawn(fut.into_actor(self).then(
            move |witnet_consensus_constants: Option<ConsensusConstants>, act, ctx| {
                act.witnet_consensus_constants = witnet_consensus_constants;
                // Wait until the function finished to schedule next call.
                // This avoids tasks running in parallel.
                ctx.run_later(period, move |act, ctx| {
                    // Reschedule check_tally_pending_drs
                    act.check_tally_pending_drs(ctx, period);
                });

                actix::fut::ready(())
            },
        ));
    }
}

/// Get network's consensus constants
async fn get_consensus_constants(
    witnet_client: Addr<JsonRpcClient>,
) -> Result<ConsensusConstants, ()> {
    let method = String::from("getConsensusConstants");
    let params = json!(null);
    let req = jsonrpc::Request::method(method)
        .timeout(Duration::from_millis(5_000))
        .params(params)
        .expect("params failed serialization");
    let result = witnet_client.send(req).await;
    let result = match result {
        Ok(result) => result,
        Err(_) => {
            log::error!("Failed to connect to witnet client, will retry later");
            return Err(());
        }
    };
    let consensus_constants = match result {
        Ok(value) => serde_json::from_value::<ConsensusConstants>(value)
            .expect("failed to deserialize consensus constants"),
        Err(e) => {
            log::error!("error in getConsensusConstants call: {:?}", e);
            return Err(());
        }
    };

    Ok(consensus_constants)
}

/// Return the timestamp of this block hash
async fn get_dr_timestamp(
    witnet_client: Addr<JsonRpcClient>,
    consensus_constants: &ConsensusConstants,
    protocol_info: &ProtocolInfo,
    drt_block_hash: Hash,
    dr_commits_round: u16,
) -> Result<i64, ()> {
    let method = String::from("getBlock");
    let params = json!([drt_block_hash]);
    let req = jsonrpc::Request::method(method)
        .timeout(Duration::from_millis(5_000))
        .params(params)
        .expect("params failed serialization");
    let report = witnet_client.send(req).await;
    let report = match report {
        Ok(report) => report,
        Err(_) => {
            log::error!("Failed to connect to witnet client, will retry later");
            return Err(());
        }
    };
    let block = match report {
        Ok(value) => serde_json::from_value::<Block>(value).expect("failed to deserialize block"),
        Err(e) => {
            log::error!("error in getBlock call ({}): {:?}", drt_block_hash, e);
            return Err(());
        }
    };
    let dr_last_commit_epoch =
        block.block_header.beacon.checkpoint + u32::from(dr_commits_round + 1);
    let protocol_version = if protocol_info
        .all_versions
        .get_activation_epoch(protocol_info.current_version)
        <= dr_last_commit_epoch
    {
        protocol_info.current_version
    } else {
        protocol_info.current_version.prev()
    };
    let protocol_activation_timestamp = protocol_info
        .derive_activation_timestamp(protocol_version, consensus_constants)
        .unwrap_or(consensus_constants.checkpoint_zero_timestamp);
    let protocol_activation_epoch = protocol_info
        .all_versions
        .get_activation_epoch(protocol_version);
    let protocol_checkpoint_period = protocol_info
        .all_checkpoints_periods
        .get(&protocol_version)
        .unwrap_or(&45u16);

    Ok(protocol_activation_timestamp
        + i64::from((dr_last_commit_epoch - protocol_activation_epoch) * u32::from(*protocol_checkpoint_period)))
}

/// Get current protocol info from the Witnet node
async fn get_protocol_info(witnet_client: Addr<JsonRpcClient>) -> Result<ProtocolInfo, ()> {
    let method = String::from("protocol");
    let params = json!(null);
    let req = jsonrpc::Request::method(method)
        .timeout(Duration::from_millis(5_000))
        .params(params)
        .expect("params failed serialization");
    let report = witnet_client.send(req).await;
    let report = match report {
        Ok(report) => report,
        Err(_) => {
            log::error!("Failed to connect to witnet client, will retry later");
            return Err(());
        }
    };
    match report {
        Ok(value) => Ok(serde_json::from_value::<ProtocolInfo>(value)
            .expect("failed to deserialize protocol info")),
        Err(e) => {
            log::error!("Error when getting protocol info: {:?}", e);
            Err(())
        }
    }
}
