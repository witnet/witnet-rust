use actix::prelude::*;
use serde::{Deserialize, Deserializer, Serialize};

 use core::fmt::Display;
 use crate::types::FromStr;

use crate::{
    actors::app,
    types::{self, Hashable as _, ProtobufConvert as _},
};
pub use witnet_data_structures::{chain::{DataRequestOutput, RADRequest}, transaction_factory::FeeType};

#[derive(Debug, Deserialize)]
pub struct CreateDataReqRequest {
    session_id: types::SessionId,
    wallet_id: String,
    #[serde(serialize_with = "into_generic_type::<_, DataRequestOutputHelper, _>", deserialize_with = "from_generic_type::<_, DataRequestOutputHelper, _>")]
    request: DataRequestOutput,
     #[serde(serialize_with = "u64_to_string", deserialize_with = "number_from_string")]
    fee: u64,
    fee_type: Option<types::FeeType>,
}

// #[derive(Debug, PartialEq, Deserialize)]
// struct RADRequest {
//     time_lock: u64,
//     retrieve: Vec<types::RADRetrieve>,
//     aggregate: types::RADAggregate,
//     tally: types::RADTally,
// }

#[derive(Debug, Serialize)]
pub struct CreateDataReqResponse {
    pub transaction_id: String,
    pub transaction: types::Transaction,
    pub bytes: String,
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


// Serialization helper

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, Hash, Default)]
struct DataRequestOutputHelper {
    pub data_request: RADRequest,
     #[serde(serialize_with = "u64_to_string", deserialize_with = "number_from_string")]
    pub witness_reward: u64,
     #[serde(serialize_with = "u16_to_string", deserialize_with = "number_from_string")]
    pub witnesses: u16,
    // This fee will be earn by the miner when include commits and/or reveals in the block
     #[serde(serialize_with = "u64_to_string", deserialize_with = "number_from_string")]
    pub commit_and_reveal_fee: u64,
    // This field must be >50 and <100.
    // >50 because simple majority
    // <100 because a 100% consensus encourages to commit a RadError for free
     #[serde(serialize_with = "u32_to_string", deserialize_with = "number_from_string")]
    pub min_consensus_percentage: u32,
    // This field must be >= collateral_minimum, or zero
    // If zero, it will be treated as collateral_minimum
    #[serde(serialize_with = "u64_to_string", deserialize_with = "number_from_string")]
    pub collateral: u64,
}

impl From<DataRequestOutput> for DataRequestOutputHelper {
    fn from(x: DataRequestOutput) -> Self {
         DataRequestOutputHelper {
                data_request: x.data_request,
                witness_reward: x.witness_reward,
                witnesses: x.witnesses,
                commit_and_reveal_fee: x.commit_and_reveal_fee,
                min_consensus_percentage: x.min_consensus_percentage,
                collateral: x.collateral,
            }
    }
}


impl From<DataRequestOutputHelper> for DataRequestOutput {
    fn from(x: DataRequestOutputHelper) -> Self {
             DataRequestOutput{
                data_request: x.data_request,
                witness_reward: x.witness_reward,
                witnesses: x.witnesses,
                commit_and_reveal_fee: x.commit_and_reveal_fee,
                min_consensus_percentage: x.min_consensus_percentage,
                collateral: x.collateral,
            }}
    
}

fn from_generic_type<'de, D, T, U>(deserializer: D) -> Result<U, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
    U: From<T>,
{
    Ok(T::deserialize(deserializer)?.into())
}

fn into_generic_type<S, U, T>(val: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: Clone,
    U: From<T>,
    U: Serialize
{
    let x = U::from(val.clone());
    x.serialize(serializer)
}

pub fn u16_to_string<S>(val: &u16, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if serializer.is_human_readable() {
        serializer.serialize_str(&val.to_string())
    } else {
        serializer.serialize_u16(*val)
    }
}

pub fn u32_to_string<S>(val: &u32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if serializer.is_human_readable() {
        serializer.serialize_str(&val.to_string())
    } else {
        serializer.serialize_u32(*val)
    }
}

pub fn u64_to_string<S>(val: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if serializer.is_human_readable() {
        serializer.serialize_str(&val.to_string())
    } else {
        serializer.serialize_u64(*val)
    }
}

pub fn number_from_string<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr + serde::Deserialize<'de>,
    <T as FromStr>::Err: Display,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrInt<T> {
        String(String),
        Number(T),
    }
    if deserializer.is_human_readable() {
        match StringOrInt::<T>::deserialize(deserializer)? {
            StringOrInt::String(s) => s.parse::<T>().map_err(serde::de::Error::custom),
            StringOrInt::Number(i) => Ok(i),
        }
    } else {
        T::deserialize(deserializer)
    }
}