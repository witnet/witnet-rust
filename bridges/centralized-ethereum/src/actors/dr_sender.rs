use crate::{
    actors::{
        dr_database::{DrDatabase, DrInfoBridge, DrState, GetAllNewDrs, SetDrInfoBridge},
        dr_reporter::{DrReporter, DrReporterMsg, Report},
    },
    config::Config,
};
use actix::prelude::*;
use serde_json::json;
use std::{fmt, time::Duration};
use witnet_config::defaults::PSEUDO_CONSENSUS_CONSTANTS_WIP0022_REWARD_COLLATERAL_RATIO;
use witnet_data_structures::{
    chain::{tapi::current_active_wips, DataRequestOutput, Hashable},
    data_request::calculate_reward_collateral_ratio,
    error::TransactionError,
    proto::ProtobufConvert,
    radon_error::RadonErrors,
    transaction::DRTransaction,
};
use witnet_net::client::tcp::{jsonrpc, JsonRpcClient};
use witnet_node::utils::stop_system_if_panicking;
use witnet_util::timestamp::get_timestamp;
use witnet_validations::validations::{validate_data_request_output, validate_rad_request};

#[cfg(test)]
mod tests;

/// DrSender actor reads the new requests from DrDatabase and includes them in Witnet
#[derive(Default)]
pub struct DrSender {
    witnet_client: Option<Addr<JsonRpcClient>>,
    witnet_dr_min_collateral_nanowits: u64,
    witnet_dr_max_value_nanowits: u64,
    witnet_dr_max_fee_nanowits: u64,
    witnet_node_pkh: Option<String>,
    polling_rate_ms: u64,
}

impl Drop for DrSender {
    fn drop(&mut self) {
        log::trace!("Dropping DrSender");
        stop_system_if_panicking("DrSender");
    }
}

/// Make actor from DrSender
impl Actor for DrSender {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("DrSender actor has been started!");

        self.check_new_drs(ctx, Duration::from_millis(self.polling_rate_ms));
    }
}

/// Required trait for being able to retrieve DrSender address from system registry
impl actix::Supervised for DrSender {}

/// Required trait for being able to retrieve DrSender address from system registry
impl SystemService for DrSender {}

impl DrSender {
    /// Initialize the `DrSender` taking the configuration from a `Config` structure
    /// and a Json-RPC client connected to a Witnet node
    pub fn from_config(config: &Config, node_client: Addr<JsonRpcClient>) -> Self {
        Self {
            polling_rate_ms: config.eth_new_drs_polling_rate_ms / 2 + 1000,
            witnet_client: Some(node_client),
            witnet_dr_min_collateral_nanowits: config.witnet_dr_min_collateral_nanowits,
            witnet_dr_max_value_nanowits: config.witnet_dr_max_value_nanowits,
            witnet_dr_max_fee_nanowits: config.witnet_dr_max_fee_nanowits,
            witnet_node_pkh: None,
        }
    }

    fn check_new_drs(&self, ctx: &mut Context<Self>, period: Duration) {
        let witnet_client = self.witnet_client.clone().unwrap();
        let witnet_dr_min_collateral_nanowits = self.witnet_dr_min_collateral_nanowits;
        let witnet_dr_max_value_nanowits = self.witnet_dr_max_value_nanowits;
        let witnet_dr_max_fee_nanowits = self.witnet_dr_max_fee_nanowits;
        let mut witnet_node_pkh = self.witnet_node_pkh.clone();

        let fut = async move {
            let dr_database_addr = DrDatabase::from_registry();
            let dr_reporter_addr = DrReporter::from_registry();

            if witnet_node_pkh.is_none() {
                // get witnet node's pkh if not yet known
                let req = jsonrpc::Request::method("getPkh").timeout(Duration::from_millis(5000));
                let res = witnet_client.send(req).await;
                witnet_node_pkh = match res {
                    Ok(Ok(res)) => match serde_json::from_value::<String>(res) {
                        Ok(pkh) => {
                            log::info!("Pkh is {}", pkh);

                            Some(pkh)
                        }
                        Err(_) => None,
                    },
                    Ok(Err(_)) => {
                        log::warn!("Cannot deserialize witnet node's pkh, will retry later");

                        None
                    }
                    Err(_) => {
                        log::warn!("Cannot get witnet node's pkh, will retry later");

                        None
                    }
                };
            } else {
                // TODO: alert if number of big enough utxos is less number of drs to broadcast
            }

            // process latest drs added or set as New in the database
            let new_drs = dr_database_addr.send(GetAllNewDrs).await.unwrap().unwrap();
            let mut dr_reporter_msgs = vec![];

            for (dr_id, dr_bytes) in new_drs {
                match deserialize_and_validate_dr_bytes(
                    &dr_bytes,
                    witnet_dr_min_collateral_nanowits,
                    witnet_dr_max_value_nanowits,
                ) {
                    Ok(dr_output) => {
                        let req = jsonrpc::Request::method("sendRequest")
                            .timeout(Duration::from_millis(5_000))
                            .params(json!({
                                "dro": dr_output, 
                                "fee": std::cmp::min(dr_output.witness_reward, witnet_dr_max_fee_nanowits)
                            }))
                            .expect("DataRequestOutput params failed serialization");
                        let res = witnet_client.send(req).await;
                        let res = match res {
                            Ok(res) => res,
                            Err(_) => {
                                log::error!("Failed to connect to witnet node, will retry later");
                                break;
                            }
                        };

                        match res {
                            Ok(dr_tx) => {
                                match serde_json::from_value::<DRTransaction>(dr_tx) {
                                    Ok(dr_tx) => {
                                        log::info!("[{}] => dr_tx = {}", dr_id, dr_tx.hash());
                                        // Save dr_tx_hash in database and set state to Pending
                                        dr_database_addr
                                            .send(SetDrInfoBridge(
                                                dr_id,
                                                DrInfoBridge {
                                                    dr_bytes,
                                                    dr_state: DrState::Pending,
                                                    dr_tx_hash: Some(dr_tx.hash()),
                                                    dr_tx_creation_timestamp: Some(get_timestamp()),
                                                },
                                            ))
                                            .await
                                            .unwrap();
                                    }
                                    Err(e) => {
                                        // Unexpected error deserializing hash
                                        panic!("[{}] >< cannot deserialize dr_tx: {}", dr_id, e);
                                    }
                                }
                            }
                            Err(e) => {
                                // Error sending transaction: node not synced, not enough balance, etc.
                                // Do nothing, will retry later.
                                log::error!("[{}] >< cannot broadcast dr_tx: {}", dr_id, e);
                                // In this case, refrain from trying to send remaining data requests, 
                                // and let the dr_sender actor try again on next poll.
                                return witnet_node_pkh;
                            }
                        }
                    }
                    Err(err) => {
                        // Error deserializing or validating data request: mark data request as
                        // error and report error as result to ethereum.
                        log::error!("[{}] >< unacceptable data request: {}", dr_id, err);
                        let result = err.encode_cbor();
                        // In this case there is no data request transaction, so
                        // we set both the dr_tx_hash and dr_tally_tx_hash to zero values.
                        let zero_hash =
                            "0000000000000000000000000000000000000000000000000000000000000000"
                                .parse()
                                .unwrap();

                        dr_reporter_msgs.push(Report {
                            dr_id,
                            dr_timestamp: i64::from_ne_bytes(get_timestamp().to_ne_bytes()),
                            dr_tx_hash: zero_hash,
                            dr_tally_tx_hash: zero_hash,
                            result,
                        });
                    }
                }
            }

            dr_reporter_addr
                .send(DrReporterMsg {
                    reports: dr_reporter_msgs,
                })
                .await
                .unwrap();

            witnet_node_pkh
        };

        ctx.spawn(fut.into_actor(self).then(move |node_pkh, _act, ctx| {
            // Wait until the function finished to schedule next call.
            // This avoids tasks running in parallel.
            ctx.run_later(period, move |act, ctx| {
                act.witnet_node_pkh = node_pkh;
                // Reschedule check_new_drs
                act.check_new_drs(ctx, period);
            });

            actix::fut::ready(())
        }));
    }
}

/// Possible reasons for why the data request has not been relayed to witnet and is resolved with
/// an error
#[derive(Debug)]
enum DrSenderError {
    /// Cannot deserialize data request bytecode as read from the WitnetOracle contract
    Deserialization { msg: String },
    /// Invalid data request SLA parameters
    Validation { msg: String },
    /// Malformed Radon script
    RadonValidation { msg: String },
    /// Invalid collateral amount
    InvalidCollateral { msg: String },
    /// E.g. the WIP-0022 reward to collateral ratio is not satisfied
    InvalidReward { msg: String },
    /// Invalid data request total value
    InvalidValue { msg: String },
    /// The cost of the data request is greater than the maximum allowed by the configuration of
    /// this bridge node
    ValueGreaterThanAllowed {
        dr_value_nanowits: u64,
        dr_max_value_nanowits: u64,
    },
}

impl fmt::Display for DrSenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DrSenderError::Deserialization { msg } => {
                write!(f, "Deserialization: {}", msg)
            }
            DrSenderError::Validation { msg } => {
                write!(f, "Validation: {}", msg)
            }
            DrSenderError::RadonValidation { msg } => {
                write!(f, "Radon validation: {}", msg)
            }
            DrSenderError::InvalidCollateral { msg } => {
                write!(f, "Invalid collateral: {}", msg)
            }
            DrSenderError::InvalidReward { msg } => {
                write!(f, "Invalid reward: {}", msg)
            }
            DrSenderError::InvalidValue { msg } => {
                write!(f, "Invalid value: {}", msg)
            }
            DrSenderError::ValueGreaterThanAllowed {
                dr_value_nanowits,
                dr_max_value_nanowits,
            } => {
                write!(
                    f,
                    "data request value ({}) higher than maximum allowed ({})",
                    dr_value_nanowits, dr_max_value_nanowits
                )
            }
        }
    }
}

impl DrSenderError {
    pub fn encode_cbor(&self) -> Vec<u8> {
        let radon_error = match self {
            // Errors for data requests that are objectively wrong
            DrSenderError::Deserialization { .. }
            | DrSenderError::Validation { .. }
            | DrSenderError::RadonValidation { .. }
            | DrSenderError::InvalidCollateral { .. }
            | DrSenderError::InvalidReward { .. }
            | DrSenderError::InvalidValue { .. } => RadonErrors::BridgeMalformedRequest,
            // Errors for data requests that the bridge node chooses not to relay, but other bridge
            // nodes may relay
            DrSenderError::ValueGreaterThanAllowed { .. } => RadonErrors::BridgePoorIncentives,
        };

        let error_code = radon_error as u8;

        // CBOR: 39([error_code])
        vec![0xD8, 0x27, 0x81, 0x18, error_code]
    }
}

fn deserialize_and_validate_dr_bytes(
    dr_bytes: &[u8],
    dr_min_collateral_nanowits: u64,
    dr_max_value_nanowits: u64,
) -> Result<DataRequestOutput, DrSenderError> {
    match DataRequestOutput::from_pb_bytes(dr_bytes) {
        Err(e) => Err(DrSenderError::Deserialization { msg: e.to_string() }),
        Ok(dr_output) => {
            let mut dr_output = dr_output;
            validate_data_request_output(
                &dr_output,
                dr_min_collateral_nanowits, // dro_hash may be altered if dr_output.collateral goes below this value
                PSEUDO_CONSENSUS_CONSTANTS_WIP0022_REWARD_COLLATERAL_RATIO,
                &current_active_wips(),
            )
            .map_err(|e| match e {
                e @ TransactionError::RewardTooLow { .. } => {
                    DrSenderError::InvalidReward { msg: e.to_string() }
                }
                e => DrSenderError::Validation { msg: e.to_string() },
            })?;

            // Collateral value validation
            if dr_output.collateral < dr_min_collateral_nanowits {
                // modify data request's collateral if below some minimum,
                // while maintaining same reward collateral ratio in such case:
                let reward_collateral_ratio = calculate_reward_collateral_ratio(
                    dr_output.collateral,
                    dr_min_collateral_nanowits,
                    dr_output.witness_reward,
                );
                let dro_hash = dr_output.hash();
                let dro_prev_collateral = dr_output.collateral;
                let dro_prev_witness_reward = dr_output.witness_reward;
                dr_output.collateral = dr_min_collateral_nanowits;
                dr_output.witness_reward = calculate_reward_collateral_ratio(
                    dr_min_collateral_nanowits,
                    dr_min_collateral_nanowits,
                    reward_collateral_ratio,
                );
                log::warn!(
                    "DRO [{}]: witnessing collateral ({}) increased to minimum ({})",
                    dro_hash,
                    dro_prev_collateral,
                    dr_min_collateral_nanowits
                );
                log::warn!(
                    "DRO [{}]: witnessing reward ({}) proportionally increased ({})",
                    dro_hash,
                    dro_prev_witness_reward,
                    dr_output.witness_reward
                )
            }
            if (dr_output.collateral != 0) && (dr_output.collateral < dr_min_collateral_nanowits) {
                return Err(DrSenderError::InvalidCollateral {
                    msg: format!(
                        "Collateral ({}) must be greater than the minimum ({})",
                        dr_output.collateral, dr_min_collateral_nanowits
                    ),
                });
            }

            validate_rad_request(&dr_output.data_request, &current_active_wips())
                .map_err(|e| DrSenderError::RadonValidation { msg: e.to_string() })?;

            // Check if we want to claim this data request:
            // Is the price ok?
            let dr_value_nanowits = dr_output
                .checked_total_value()
                .map_err(|e| DrSenderError::InvalidValue { msg: e.to_string() })?;
            if dr_value_nanowits > dr_max_value_nanowits {
                return Err(DrSenderError::ValueGreaterThanAllowed {
                    dr_value_nanowits,
                    dr_max_value_nanowits,
                });
            }

            Ok(dr_output)
        }
    }
}
