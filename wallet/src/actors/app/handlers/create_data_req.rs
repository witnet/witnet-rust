use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;
use crate::types::{Hashable as _, ProtobufConvert as _};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDataReqRequest {
    session_id: types::SessionId,
    wallet_id: String,
    label: Option<String>,
    fee: u64,
    request: DataRequestOutput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DataRequestOutput {
    data_request: RADRequest,
    witness_reward: u64,
    witnesses: u16,
    backup_witnesses: u16,
    commit_fee: u64,
    reveal_fee: u64,
    tally_fee: u64,
    extra_commit_rounds: u16,
    extra_reveal_rounds: u16,
    min_consensus_percentage: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RADRequest {
    time_lock: u64,
    retrieve: Vec<types::RADRetrieve>,
    aggregate: types::RADAggregate,
    tally: types::RADTally,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDataReqResponse {
    pub transaction_id: String,
    pub transaction: types::Transaction,
    pub bytes: String,
}

impl Message for CreateDataReqRequest {
    type Result = app::Result<CreateDataReqResponse>;
}

impl Handler<CreateDataReqRequest> for app::App {
    type Result = app::ResponseActFuture<CreateDataReqResponse>;

    fn handle(
        &mut self,
        CreateDataReqRequest {
            request,
            label,
            fee,
            session_id,
            wallet_id,
        }: CreateDataReqRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let validated = validate(request).map_err(app::validation_error);

        let f = fut::result(validated).and_then(move |request, slf: &mut Self, _ctx| {
            let params = types::DataReqParams {
                request,
                fee,
                label,
            };

            slf.create_data_req(&session_id, &wallet_id, params)
                .map(|transaction, _, _| {
                    let transaction_id = hex::encode(transaction.hash().as_ref());
                    let bytes = hex::encode(transaction.to_pb_bytes().unwrap());

                    CreateDataReqResponse {
                        transaction_id,
                        transaction,
                        bytes,
                    }
                })
        });

        Box::new(f)
    }
}

/// Validate `CreateDataReqRequest`.
///
/// To be valid it must pass these checks:
/// - value is greater that the sum of `witnesses` times the sum of the fees
/// - value minus all the fees must divisible by the number of witnesses
fn validate(request: DataRequestOutput) -> Result<types::DataRequestOutput, app::ValidationErrors> {
    let req = types::DataRequestOutput {
        data_request: types::RADRequest {
            time_lock: request.data_request.time_lock,
            retrieve: request.data_request.retrieve,
            aggregate: request.data_request.aggregate,
            tally: request.data_request.tally,
        },
        witness_reward: request.witness_reward,
        witnesses: request.witnesses,
        backup_witnesses: request.backup_witnesses,
        commit_fee: request.commit_fee,
        reveal_fee: request.reveal_fee,
        tally_fee: request.tally_fee,
        extra_commit_rounds: request.extra_commit_rounds,
        extra_reveal_rounds: request.extra_reveal_rounds,
        min_consensus_percentage: request.min_consensus_percentage,
    };

    let request = witnet_validations::validations::validate_data_request_output(&req)
        .map_err(|err| app::field_error("request", format!("{}", err)));

    let data_request = witnet_validations::validations::validate_rad_request(&req.data_request)
        .map_err(|err| app::field_error("dataRequest", format!("{}", err)));

    app::combine_field_errors(request, data_request, move |_, _| req)
}
