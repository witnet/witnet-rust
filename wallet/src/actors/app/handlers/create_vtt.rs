use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;
use crate::types::{Hashable as _, ProtobufConvert as _};

use witnet_data_structures::chain::Environment;

#[derive(Debug, Serialize, Deserialize)]
pub struct VttOutputParams {
    pub address: String,
    pub amount: u64,
    pub time_lock: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateVttRequest {
    fee: u64,
    label: Option<String>,
    outputs: Vec<VttOutputParams>,
    session_id: types::SessionId,
    wallet_id: String,
    weighted_fee: Option<u64>,
}

/// Part of CreateVttResponse struct, containing additional data to be displayed in clients
/// (e.g. in a confirmation screen)
#[derive(Debug, Serialize)]
pub struct VttMetadata {
    fee: u64,
    outputs: Vec<VttOutputParams>,
}

#[derive(Debug, Serialize)]
pub struct CreateVttResponse {
    pub transaction_id: String,
    pub transaction: types::Transaction,
    pub bytes: String,
    pub metadata: VttMetadata,
}

impl Message for CreateVttRequest {
    type Result = app::Result<CreateVttResponse>;
}

impl Handler<CreateVttRequest> for app::App {
    type Result = app::ResponseActFuture<CreateVttResponse>;

    fn handle(&mut self, msg: CreateVttRequest, _ctx: &mut Self::Context) -> Self::Result {
        let testnet = self.params.testnet;
        let validated =
            validate_output_addresses(testnet, &msg.outputs).map_err(app::validation_error);

        let f = fut::result(validated).and_then(move |outputs, act: &mut Self, _ctx| {
            let params = types::VttParams {
                fee: msg.fee,
                outputs,
                weighted_fee: msg.weighted_fee,
            };

            act.create_vtt(&msg.session_id, &msg.wallet_id, params)
                .map(|transaction, _, _| {
                    let transaction_id = hex::encode(transaction.hash().as_ref());
                    let bytes = hex::encode(transaction.to_pb_bytes().unwrap());

                    CreateVttResponse {
                        transaction_id,
                        transaction,
                        bytes,
                        metadata: VttMetadata {
                            fee: msg.fee,
                            outputs: msg.outputs,
                        },
                    }
                })
                .map_err(|err, _, _| {
                    log::error!("Failed to create a VTT: {}", err);

                    err
                })
        });

        Box::new(f)
    }
}

/// Validate output addresses and transform addresses to `ValueTransferOutputs`
///
/// To be valid it must pass these checks:
/// - destination address must be in the same network (testnet/mainnet)
fn validate_output_addresses(
    testnet: bool,
    outputs: &[VttOutputParams],
) -> Result<Vec<types::ValueTransferOutput>, app::ValidationErrors> {
    let environment = if testnet {
        Environment::Testnet
    } else {
        Environment::Mainnet
    };
    outputs.iter().try_fold(vec![], |mut acc, output| {
        types::PublicKeyHash::from_bech32(environment, &output.address)
            .map(|pkh| {
                acc.push(types::ValueTransferOutput {
                    pkh,
                    value: output.amount,
                    time_lock: output.time_lock.unwrap_or_default(),
                });
            })
            .map_err(|err| {
                log::warn!("Invalid address: {}", err);

                app::field_error("address", "Address failed to deserialize.")
            })?;

        Ok(acc)
    })
}

#[cfg(test)]
use witnet_data_structures::chain::{PublicKeyHash, ValueTransferOutput};

#[test]
fn test_validate_addresses() {
    let output_mainnet = [VttOutputParams {
        address: "wit18cfejmk3305y9kw5xqa59rwnpjzahr57us48vm".to_string(),
        amount: 10,
        time_lock: None,
    }];
    let validation = validate_output_addresses(false, &output_mainnet).unwrap();
    assert_eq!(
        validation,
        [ValueTransferOutput {
            pkh: PublicKeyHash::from_bech32(
                Environment::Mainnet,
                "wit18cfejmk3305y9kw5xqa59rwnpjzahr57us48vm",
            )
            .unwrap(),
            value: 10,
            time_lock: 0,
        }]
    );

    let output_testnet = [VttOutputParams {
        address: "twit1adgt8t2h3xnu358f76zxlph0urf2ev7cd78ggc".to_string(),
        amount: 10,
        time_lock: None,
    }];
    let validation = validate_output_addresses(true, &output_testnet).unwrap();
    assert_eq!(
        validation,
        [ValueTransferOutput {
            pkh: PublicKeyHash::from_bech32(
                Environment::Testnet,
                "twit1adgt8t2h3xnu358f76zxlph0urf2ev7cd78ggc",
            )
            .unwrap(),
            value: 10,
            time_lock: 0,
        }]
    );
}

#[test]
fn test_validate_addresses_errors() {
    let output_wrong_address = [VttOutputParams {
        address: "wit18cfejmk3305y9kw5xqa59rwnpjzahr57us48vx".to_string(),
        amount: 10,
        time_lock: None,
    }];
    assert!(validate_output_addresses(false, &output_wrong_address).is_err());

    let output_mainnet = [VttOutputParams {
        address: "wit18cfejmk3305y9kw5xqa59rwnpjzahr57us48vm".to_string(),
        amount: 10,
        time_lock: None,
    }];
    assert!(validate_output_addresses(true, &output_mainnet).is_err());

    let output_testnet = [VttOutputParams {
        address: "twit1adgt8t2h3xnu358f76zxlph0urf2ev7cd78ggc".to_string(),
        amount: 10,
        time_lock: None,
    }];
    assert!(validate_output_addresses(false, &output_testnet).is_err());
}
