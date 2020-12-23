use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    actors::app,
    types::{
        self, from_generic_type, into_generic_type, number_from_string, u64_to_string,
        DataRequestOutputHelper, Hashable as _, ProtobufConvert as _, TransactionHelper,
    },
};
use witnet_data_structures::{
    chain::DataRequestOutput, transaction::Transaction, transaction_factory::FeeType,
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
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    fee: u64,
    fee_type: Option<types::FeeType>,
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
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub fee: u64,
}

impl Message for CreateDataReqRequest {
    type Result = app::Result<CreateDataReqResponse>;
}

impl Handler<CreateDataReqRequest> for app::App {
    type Result = app::ResponseActFuture<CreateDataReqResponse>;

    fn handle(&mut self, msg: CreateDataReqRequest, _ctx: &mut Self::Context) -> Self::Result {
        let validated = validate(msg.request.clone()).map_err(app::validation_error);

        let fee_type = msg.fee_type.unwrap_or(FeeType::Weighted);

        let f = fut::result(validated).and_then(move |request, slf: &mut Self, _ctx| {
            let params = types::DataReqParams {
                request,
                fee: msg.fee,
                fee_type,
            };

            slf.create_data_req(&msg.session_id, &msg.wallet_id, params)
                .map(move |transaction, _, _| {
                    let fee = match fee_type {
                        FeeType::Absolute => msg.fee,
                        FeeType::Weighted => msg.fee * u64::from(transaction.weight()),
                    };

                    let transaction_id = hex::encode(transaction.hash().as_ref());
                    let bytes = hex::encode(transaction.to_pb_bytes().unwrap());

                    CreateDataReqResponse {
                        transaction_id,
                        transaction,
                        bytes,
                        fee,
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
