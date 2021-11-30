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
use witnet_data_structures::{
    chain::{DataRequestOutput, Hash},
    mainnet_validations::current_active_wips,
    proto::ProtobufConvert,
    radon_error::RadonErrors,
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
    wit_dr_sender_polling_rate_ms: u64,
    max_dr_value_nanowits: u64,
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

        self.check_new_drs(
            ctx,
            Duration::from_millis(self.wit_dr_sender_polling_rate_ms),
        );
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
        let max_dr_value_nanowits = config.max_dr_value_nanowits;
        let wit_dr_sender_polling_rate_ms = config.wit_dr_sender_polling_rate_ms;

        Self {
            witnet_client: Some(node_client),
            wit_dr_sender_polling_rate_ms,
            max_dr_value_nanowits,
        }
    }

    fn check_new_drs(&self, ctx: &mut Context<Self>, period: Duration) {
        let witnet_client = self.witnet_client.clone().unwrap();
        let max_dr_value_nanowits = self.max_dr_value_nanowits;

        let fut = async move {
            let dr_database_addr = DrDatabase::from_registry();
            let dr_reporter_addr = DrReporter::from_registry();

            let new_drs = dr_database_addr.send(GetAllNewDrs).await.unwrap().unwrap();
            let mut dr_reporter_msgs = vec![];

            for (dr_id, dr_bytes) in new_drs {
                match deserialize_and_validate_dr_bytes(&dr_bytes, max_dr_value_nanowits) {
                    Ok(dr_output) => {
                        let req = jsonrpc::Request::method("sendRequest")
                            .timeout(Duration::from_millis(5_000))
                            .params(json!({"dro": dr_output, "fee": 10_000}))
                            .expect("params failed serialization");
                        let res = witnet_client.send(req).await;
                        let res = match res {
                            Ok(res) => res,
                            Err(_) => {
                                log::error!("Failed to connect to witnet client, will retry later");
                                break;
                            }
                        };

                        match res {
                            Ok(dr_tx_hash) => {
                                match serde_json::from_value::<Hash>(dr_tx_hash) {
                                    Ok(dr_tx_hash) => {
                                        // Save dr_tx_hash in database and set state to Pending
                                        dr_database_addr
                                            .send(SetDrInfoBridge(
                                                dr_id,
                                                DrInfoBridge {
                                                    dr_bytes,
                                                    dr_state: DrState::Pending,
                                                    dr_tx_hash: Some(dr_tx_hash),
                                                    dr_tx_creation_timestamp: Some(get_timestamp()),
                                                },
                                            ))
                                            .await
                                            .unwrap();
                                    }
                                    Err(e) => {
                                        // Unexpected error deserializing hash
                                        panic!("[{}] error deserializing dr_tx_hash: {}", dr_id, e);
                                    }
                                }
                            }
                            Err(e) => {
                                // Error sending transaction: node not synced, not enough balance, etc.
                                // Do nothing, will retry later.
                                log::error!(
                                    "[{}] error creating data request transaction: {}",
                                    dr_id,
                                    e
                                );
                                continue;
                            }
                        }
                    }
                    Err(err) => {
                        // Error deserializing or validating data request: mark data request as
                        // error and report error as result to ethereum.
                        log::error!("[{}] error: {}", dr_id, err);
                        let result = err.encode_cbor();
                        // In this case there is no data request transaction, so the dr_tx_hash
                        // field can be set to anything.
                        // Except all zeros, because that hash is invalid.
                        let dr_tx_hash =
                            "0000000000000000000000000000000000000000000000000000000000000001"
                                .parse()
                                .unwrap();

                        dr_reporter_msgs.push(Report {
                            dr_id,
                            timestamp: 0,
                            dr_tx_hash,
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
        };

        ctx.spawn(fut.into_actor(self).then(move |(), _act, ctx| {
            // Wait until the function finished to schedule next call.
            // This avoids tasks running in parallel.
            ctx.run_later(period, move |act, ctx| {
                // Reschedule check_new_drs
                act.check_new_drs(ctx, period);
            });

            actix::fut::ready(())
        }));
    }
}

/// Possible reasons for why the data request has not been relayed to witnet and is resolved with
/// an error
enum DrSenderError {
    /// The data request bytes are not a valid DataRequestOutput
    Deserialization { msg: String },
    /// The DataRequestOutput is invalid (wrong number of witnesses, wrong min_consensus_percentage)
    Validation { msg: String },
    /// The RADRequest is invalid (malformed radon script)
    RadonValidation { msg: String },
    /// The specified collateral amount is invalid
    InvalidCollateral { msg: String },
    /// Overflow when calculating the data request value
    InvalidValue { msg: String },
    /// The cost of the data request is greater than the maximum allowed by the configuration of
    /// this bridge node
    ValueGreaterThanAllowed {
        dr_value_nanowits: u64,
        max_dr_value_nanowits: u64,
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
            DrSenderError::InvalidValue { msg } => {
                write!(f, "Invalid value: {}", msg)
            }
            DrSenderError::ValueGreaterThanAllowed {
                dr_value_nanowits,
                max_dr_value_nanowits,
            } => {
                write!(
                    f,
                    "data request value ({}) higher than maximum allowed ({})",
                    dr_value_nanowits, max_dr_value_nanowits
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
            | DrSenderError::InvalidValue { .. } => RadonErrors::BridgeMalformedRequest,
            // Errors for data requests that the bridge node chooses not to relay, but other bridge
            // nodes may relay
            DrSenderError::InvalidCollateral { .. }
            | DrSenderError::ValueGreaterThanAllowed { .. } => RadonErrors::BridgePoorIncentives,
        };

        let error_code = radon_error as u8;

        // CBOR: 39([error_code])
        vec![0xD8, 0x27, 0x81, 0x18, error_code]
    }
}

fn deserialize_and_validate_dr_bytes(
    dr_bytes: &[u8],
    max_dr_value_nanowits: u64,
) -> Result<DataRequestOutput, DrSenderError> {
    match DataRequestOutput::from_pb_bytes(dr_bytes) {
        Err(e) => Err(DrSenderError::Deserialization { msg: e.to_string() }),
        Ok(dr_output) => {
            validate_data_request_output(&dr_output)
                .map_err(|e| DrSenderError::Validation { msg: e.to_string() })?;

            // TODO: read collateral minimum from consensus constants
            let collateral_minimum = 1;
            // Collateral value validation
            // If collateral is equal to 0 means that is equal to collateral_minimum value
            if (dr_output.collateral != 0) && (dr_output.collateral < collateral_minimum) {
                return Err(DrSenderError::InvalidCollateral {
                    msg: format!(
                        "Collateral ({}) must be greater than the minimum ({})",
                        dr_output.collateral, collateral_minimum
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
            if dr_value_nanowits > max_dr_value_nanowits {
                return Err(DrSenderError::ValueGreaterThanAllowed {
                    dr_value_nanowits,
                    max_dr_value_nanowits,
                });
            }

            Ok(dr_output)
        }
    }
}
