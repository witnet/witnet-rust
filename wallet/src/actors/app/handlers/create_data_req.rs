use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    actors::app,
    types::{self, Hashable as _, ProtobufConvert as _},
};
use witnet_data_structures::{chain::DataRequestOutput, transaction_factory::FeeType};

#[derive(Debug, Deserialize)]
pub struct CreateDataReqRequest {
    session_id: types::SessionId,
    wallet_id: String,
    request: DataRequestOutput,
    fee: u64,
    fee_type: Option<types::FeeType>,
}

#[derive(Debug, Deserialize)]
struct RADRequest {
    time_lock: u64,
    retrieve: Vec<types::RADRetrieve>,
    aggregate: types::RADAggregate,
    tally: types::RADTally,
}

#[derive(Debug, Serialize)]
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

    fn handle(&mut self, msg: CreateDataReqRequest, _ctx: &mut Self::Context) -> Self::Result {
        let validated = validate(msg.request.clone()).map_err(app::validation_error);

        let f = fut::result(validated).and_then(move |request, slf: &mut Self, _ctx| {
            let params = types::DataReqParams {
                request,
                fee: msg.fee,
                fee_type: msg.fee_type.unwrap_or(FeeType::Weighted),
            };

            slf.create_data_req(&msg.session_id, &msg.wallet_id, params)
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
    let req = request;

    let request = witnet_validations::validations::validate_data_request_output(&req)
        .map_err(|err| app::field_error("request", format!("{}", err)));

    let data_request = witnet_validations::validations::validate_rad_request(&req.data_request)
        .map_err(|err| app::field_error("dataRequest", format!("{}", err)));

    app::combine_field_errors(request, data_request, move |_, _| req)
}
