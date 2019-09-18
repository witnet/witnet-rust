use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;
use crate::types::{Hashable as _, ProtobufConvert as _};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDataReqRequest {
    session_id: types::SessionId,
    wallet_id: String,
    label: Option<String>,
    request: types::DataRequestOutput,
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

    fn handle(&mut self, msg: CreateDataReqRequest, _ctx: &mut Self::Context) -> Self::Result {
        let validated = validate(&msg.request).map_err(app::validation_error);

        let f = fut::result(validated).and_then(|request, slf: &mut Self, _ctx| {
            let params = types::DataReqParams {
                request: msg.request,
                label: msg.label,
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
fn validate(request: &types::DataRequestOutput) -> Result<(), app::ValidationErrors> {
    witnet_validations::validations::validate_data_request_output(request)
        .map_err(|err| app::field_error("request", format!("{}", err)))
}
