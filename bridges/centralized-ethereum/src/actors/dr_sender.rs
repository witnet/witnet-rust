use crate::{
    actors::{
        dr_database::{DrDatabase, DrInfoBridge, DrState, GetAllNewDrs, SetDrInfoBridge},
        dr_reporter::{DrReporter, DrReporterMsg},
    },
    config::Config,
};
use actix::prelude::*;
use async_jsonrpc_client::{
    transports::{shared::EventLoopHandle, tcp::TcpSocket},
    Transport,
};
use futures_util::compat::Compat01As03;
use serde_json::json;
use std::{fmt, sync::Arc, time::Duration};
use witnet_data_structures::{
    chain::{DataRequestOutput, Hash},
    proto::ProtobufConvert,
};
use witnet_validations::validations::{validate_data_request_output, validate_rad_request};

/// DrSender actor reads the new requests from DrDatabase and includes them in Witnet
#[derive(Default)]
pub struct DrSender {
    witnet_client: Option<Arc<TcpSocket>>,
    _handle: Option<EventLoopHandle>,
    wit_dr_sender_polling_rate_ms: u64,
    max_dr_value_nanowits: u64,
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
    /// Initialize `PeersManager` taking the configuration from a `Config` structure
    pub fn from_config(config: &Config) -> Result<Self, String> {
        let max_dr_value_nanowits = config.max_dr_value_nanowits;
        let wit_dr_sender_polling_rate_ms = config.wit_dr_sender_polling_rate_ms;
        let witnet_addr = config.witnet_jsonrpc_addr.to_string();

        let (_handle, witnet_client) = TcpSocket::new(&witnet_addr).unwrap();
        let witnet_client = Arc::new(witnet_client);

        Ok(Self {
            witnet_client: Some(witnet_client),
            _handle: Some(_handle),
            wit_dr_sender_polling_rate_ms,
            max_dr_value_nanowits,
        })
    }

    fn check_new_drs(&self, ctx: &mut Context<Self>, period: Duration) {
        let witnet_client = self.witnet_client.clone().unwrap();
        let max_dr_value_nanowits = self.max_dr_value_nanowits;

        let fut = async move {
            let dr_database_addr = DrDatabase::from_registry();
            let dr_reporter_addr = DrReporter::from_registry();

            let new_drs = dr_database_addr.send(GetAllNewDrs).await.unwrap().unwrap();

            for (dr_id, dr_bytes) in new_drs {
                match deserialize_and_validate_dr_bytes(&dr_bytes, max_dr_value_nanowits) {
                    Ok(dr_output) => {
                        let bdr_params = json!({"dro": dr_output, "fee": 0});
                        let res = witnet_client.execute("sendRequest", bdr_params);
                        let res = Compat01As03::new(res).await;

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

                        // TODO: review if dr_tx_hash = [0;32] makes sense
                        dr_reporter_addr
                            .send(DrReporterMsg {
                                dr_id,
                                dr_bytes,
                                dr_tx_hash: Default::default(),
                                result,
                            })
                            .await
                            .unwrap();
                    }
                }
            }
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
        // TODO: decide on error result, currently using vec with one element [0]
        // This cannot be an empty vector because an empty vector means that the
        // request has not finished yet.
        // TODO: return serialized radon error, vec![0] is wrong because it is a valid value:
        // integer(0)
        vec![0]
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

            validate_rad_request(&dr_output.data_request)
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
