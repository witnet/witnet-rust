use std::collections::HashSet;

use actix::prelude::*;
use serde::{Deserialize, Serialize};
use witnet_data_structures::{
    chain::{Environment, Hashable, OutputPointer, PublicKeyHash, ValueTransferOutput},
    fee::{deserialize_fee_backwards_compatible, AbsoluteFee, Fee},
    proto::ProtobufConvert,
    transaction::Transaction,
    utxo_pool::UtxoSelectionStrategy,
};

use crate::{
    actors::{app, worker},
    model::TransactionMetadata,
    types::{
        self, fee_compat, from_generic_type, from_generic_type_vec, into_generic_type,
        into_generic_type_vec, number_from_string, u32_to_string, FeeType, TransactionHelper,
        VttOutputParamsHelper,
    },
};
use itertools::Itertools;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VttOutputParams {
    pub address: String,
    pub amount: u64,
    pub time_lock: Option<u64>,
}

impl From<ValueTransferOutput> for VttOutputParams {
    fn from(
        ValueTransferOutput {
            pkh,
            value,
            time_lock,
        }: ValueTransferOutput,
    ) -> Self {
        Self {
            address: pkh.to_string(),
            amount: value,
            time_lock: if time_lock == 0 {
                None
            } else {
                Some(time_lock)
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateVttRequest {
    #[serde(deserialize_with = "deserialize_fee_backwards_compatible")]
    fee: Fee,
    fee_type: Option<FeeType>,
    label: Option<String>,
    #[serde(
        serialize_with = "into_generic_type_vec::<_, VttOutputParamsHelper, _>",
        deserialize_with = "from_generic_type_vec::<_, VttOutputParamsHelper, _>"
    )]
    outputs: Vec<VttOutputParams>,
    session_id: types::SessionId,
    wallet_id: String,
    #[serde(default)]
    utxo_strategy: UtxoSelectionStrategy,
    #[serde(default)]
    selected_utxos: HashSet<OutputPointer>,
}

/// Part of CreateVttResponse struct, containing additional data to be displayed in clients
/// (e.g. in a confirmation screen)
#[derive(Debug, Serialize, Deserialize)]
pub struct VttMetadata {
    #[serde(deserialize_with = "number_from_string")]
    fee: AbsoluteFee,
    #[serde(
        serialize_with = "into_generic_type_vec::<_, VttOutputParamsHelper, _>",
        deserialize_with = "from_generic_type_vec::<_, VttOutputParamsHelper, _>"
    )]
    inputs: Vec<VttOutputParams>,
    #[serde(
        serialize_with = "into_generic_type_vec::<_, VttOutputParamsHelper, _>",
        deserialize_with = "from_generic_type_vec::<_, VttOutputParamsHelper, _>"
    )]
    outputs: Vec<VttOutputParams>,
    #[serde(
        serialize_with = "u32_to_string",
        deserialize_with = "number_from_string"
    )]
    weight: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateVttResponse {
    pub transaction_id: String,
    #[serde(
        serialize_with = "into_generic_type::<_, TransactionHelper, _>",
        deserialize_with = "from_generic_type::<_, TransactionHelper, _>"
    )]
    pub transaction: Transaction,
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

        // For the sake of backwards compatibility, if the `fee_type` argument was provided, then we
        // treat the `fee` argument as such type, regardless of how it was originally deserialized.
        let fee = fee_compat(msg.fee, msg.fee_type);

        let f = fut::result(validated).and_then(move |outputs, act: &mut Self, _ctx| {
            let params = types::VttParams {
                fee,
                outputs,
                utxo_strategy: msg.utxo_strategy.clone(),
                selected_utxos: msg.selected_utxos.iter().map(|x| x.into()).collect(),
            };

            act.create_vtt(&msg.session_id, &msg.wallet_id, params)
                .map_ok(
                    move |worker::CreateVttResponse { fee, transaction }, _, _| {
                        let inputs = match transaction.metadata {
                            Some(TransactionMetadata::InputValues(inputs)) => {
                                inputs.into_iter().map(From::from).collect_vec()
                            }
                            _ => vec![],
                        };
                        let transaction = transaction.transaction;
                        let transaction_id = hex::encode(transaction.hash().as_ref());
                        let bytes = hex::encode(transaction.to_pb_bytes().unwrap());
                        let weight = transaction.weight();

                        CreateVttResponse {
                            transaction_id,
                            transaction,
                            bytes,
                            metadata: VttMetadata {
                                fee,
                                inputs,
                                outputs: msg.outputs,
                                weight,
                            },
                        }
                    },
                )
                .map_err(|err, _, _| {
                    log::error!("Failed to create a VTT: {}", err);

                    err
                })
        });

        Box::pin(f)
    }
}

/// Validate output addresses and transform addresses to `ValueTransferOutputs`
///
/// To be valid it must pass these checks:
/// - destination address must be in the same network (testnet/mainnet)
pub fn validate_output_addresses(
    testnet: bool,
    outputs: &[VttOutputParams],
) -> Result<Vec<ValueTransferOutput>, app::ValidationErrors> {
    let environment = if testnet {
        Environment::Testnet
    } else {
        Environment::Mainnet
    };
    outputs.iter().try_fold(vec![], |mut acc, output| {
        PublicKeyHash::from_bech32(environment, &output.address)
            .map(|pkh| {
                acc.push(ValueTransferOutput {
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
mod tests {
    use witnet_data_structures::chain::{Environment, PublicKeyHash, ValueTransferOutput};

    use crate::actors::app::{validate_output_addresses, VttOutputParams};

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
}
