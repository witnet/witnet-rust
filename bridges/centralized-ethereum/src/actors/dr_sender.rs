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
use std::{sync::Arc, time::Duration};
use web3::ethabi::Bytes;
use witnet_data_structures::{
    chain::{DataRequestOutput, Hash},
    proto::ProtobufConvert,
};
use witnet_validations::validations::validate_rad_request;

/// EthPoller (TODO: Explanation)
#[derive(Default)]
pub struct DrSender {
    witnet_client: Option<Arc<TcpSocket>>,
    _handle: Option<EventLoopHandle>,
    wit_dr_sender_polling_rate_ms: u64,
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
        let wit_dr_sender_polling_rate_ms = config.wit_dr_sender_polling_rate_ms;
        let witnet_addr = config.witnet_jsonrpc_addr.to_string();

        let (_handle, witnet_client) = TcpSocket::new(&witnet_addr).unwrap();
        let witnet_client = Arc::new(witnet_client);

        Ok(Self {
            witnet_client: Some(witnet_client),
            _handle: Some(_handle),
            wit_dr_sender_polling_rate_ms,
        })
    }

    fn check_new_drs(&self, ctx: &mut Context<Self>, period: Duration) {
        let witnet_client = self.witnet_client.clone().unwrap();

        let fut = async move {
            let dr_database_addr = DrDatabase::from_registry();
            let dr_reporter_addr = DrReporter::from_registry();

            let new_drs = dr_database_addr.send(GetAllNewDrs).await.unwrap().unwrap();

            for (dr_id, dr_bytes) in new_drs {
                match deserialize_and_validate_dr_bytes(&dr_bytes) {
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
                    Err(e) => {
                        // Error deserializing or validating data request: mark data request as
                        // error and report error as result to ethereum.
                        log::error!("[{}] error deserializing data request: {}", dr_id, e);
                        // TODO: decide on error result, currently using vec with one element [0]
                        // This cannot be an empty vector because an empty vector means that the
                        // request has not finished yet.
                        let result = vec![0];

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

        ctx.spawn(fut.into_actor(self));

        ctx.run_later(period, move |act, ctx| {
            act.check_new_drs(ctx, period);
        });
    }
}

fn deserialize_and_validate_dr_bytes(dr_bytes: &Bytes) -> Result<DataRequestOutput, String> {
    match DataRequestOutput::from_pb_bytes(&dr_bytes) {
        Ok(dr) => {
            validate_rad_request(&dr.data_request)
                .map_err(|e| format!("Error validating data request: {}", e))?;
            // TODO: check if we want to claim this data request:
            // Is the price ok?

            Ok(dr)
        }
        Err(e) => Err(format!("Error deserializing data request: {}", e)),
    }
}
