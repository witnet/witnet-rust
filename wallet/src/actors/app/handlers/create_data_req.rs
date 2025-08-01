use actix::prelude::*;
use serde::{Deserialize, Serialize};
use witnet_config::defaults::PSEUDO_CONSENSUS_CONSTANTS_WIP0022_REWARD_COLLATERAL_RATIO;
use witnet_data_structures::{
    chain::{DataRequestOutput, tapi::current_active_wips},
    fee::{AbsoluteFee, Fee, deserialize_fee_backwards_compatible},
    proto::{
        ProtobufConvert,
        versioning::{ProtocolVersion, VersionedHashable},
    },
    serialization_helpers::number_from_string,
    transaction::Transaction,
};

use crate::{
    actors::{
        app::{self, handlers::create_vtt::VttOutputParams},
        worker,
    },
    model::TransactionMetadata,
    types::{
        self, DataRequestOutputHelper, FeeType, TransactionHelper, VttOutputParamsHelper,
        fee_compat, from_generic_type, from_generic_type_vec, into_generic_type,
        into_generic_type_vec, u32_to_string,
    },
};

#[derive(Debug, Deserialize)]
pub struct CreateDataReqRequest {
    session_id: types::SessionId,
    wallet_id: String,
    #[serde(
        serialize_with = "into_generic_type::<_, DataRequestOutputHelper, _>",
        deserialize_with = "from_generic_type::<_, DataRequestOutputHelper, _>"
    )]
    request: DataRequestOutput,
    #[serde(deserialize_with = "deserialize_fee_backwards_compatible")]
    fee: Fee,
    fee_type: Option<FeeType>,
    #[serde(default)]
    preview: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateDataReqResponse {
    pub transaction_id: String,
    #[serde(
        serialize_with = "into_generic_type::<_, TransactionHelper, _>",
        deserialize_with = "from_generic_type::<_, TransactionHelper, _>"
    )]
    pub transaction: Transaction,
    pub bytes: String,
    #[serde(deserialize_with = "number_from_string")]
    pub fee: AbsoluteFee,
    #[serde(
        serialize_with = "u32_to_string",
        deserialize_with = "number_from_string"
    )]
    weight: u32,
    #[serde(
        serialize_with = "into_generic_type_vec::<_, VttOutputParamsHelper, _>",
        deserialize_with = "from_generic_type_vec::<_, VttOutputParamsHelper, _>"
    )]
    inputs: Vec<VttOutputParams>,
}

impl Message for CreateDataReqRequest {
    type Result = app::Result<CreateDataReqResponse>;
}

impl Handler<CreateDataReqRequest> for app::App {
    type Result = app::ResponseActFuture<CreateDataReqResponse>;

    fn handle(&mut self, msg: CreateDataReqRequest, _ctx: &mut Self::Context) -> Self::Result {
        let consensus_constants = &self.params.consensus_constants;
        let required_reward_collateral_ratio =
            PSEUDO_CONSENSUS_CONSTANTS_WIP0022_REWARD_COLLATERAL_RATIO;
        let validated = validate(
            msg.request.clone(),
            consensus_constants.collateral_minimum,
            required_reward_collateral_ratio,
        )
        .map_err(app::validation_error);

        // For the sake of backwards compatibility, if the `fee_type` argument was provided, then we
        // treat the `fee` argument as such type, regardless of how it was originally deserialized.
        let fee = fee_compat(msg.fee, msg.fee_type);

        let f = fut::result(validated).and_then(move |request, slf: &mut Self, _ctx| {
            let params = types::DataReqParams {
                request,
                fee,
                preview: msg.preview,
            };

            slf.create_data_req(&msg.session_id, &msg.wallet_id, params)
                .map_ok(
                    move |worker::CreateDataReqResponse { fee, transaction }, _, _| {
                        let inputs = match transaction.metadata {
                            Some(TransactionMetadata::InputValues(inputs)) => {
                                inputs.into_iter().map(From::from).collect()
                            }
                            _ => vec![],
                        };
                        let transaction = transaction.transaction;
                        let transaction_id =
                            hex::encode(transaction.versioned_hash(ProtocolVersion::V2_0).as_ref());
                        let bytes = hex::encode(transaction.to_pb_bytes().unwrap());
                        let weight = transaction.weight();

                        CreateDataReqResponse {
                            transaction_id,
                            transaction,
                            bytes,
                            fee,
                            weight,
                            inputs,
                        }
                    },
                )
        });

        Box::pin(f)
    }
}

/// Validate `CreateDataReqRequest`.
///
/// To be valid it must pass these checks:
/// - value is greater that the sum of `witnesses` times the sum of the fees
/// - value minus all the fees must divisible by the number of witnesses
fn validate(
    request: DataRequestOutput,
    minimum_collateral: u64,
    required_reward_collateral_ratio: u64,
) -> Result<DataRequestOutput, app::ValidationErrors> {
    let req = request;

    let request = witnet_validations::validations::validate_data_request_output(
        &req,
        minimum_collateral,
        required_reward_collateral_ratio,
        &current_active_wips(),
    )
    .map_err(|err| app::field_error("request", format!("{err}")));

    let data_request = witnet_validations::validations::validate_rad_request(
        &req.data_request,
        &current_active_wips(),
    )
    .map_err(|err| app::field_error("dataRequest", format!("{err}")));

    app::combine_field_errors(request, data_request, move |_, _| req)
}
