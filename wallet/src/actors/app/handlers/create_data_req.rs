use actix::prelude::*;
use core::fmt::Display;
use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    actors::app,
    types::{self, FromStr, Hashable as _, ProtobufConvert as _},
};
pub use witnet_data_structures::{
    chain::{
        DataRequestOutput, Input, KeyedSignature, PublicKeyHash, RADRequest, ValueTransferOutput,
    },
    transaction::{
        CommitTransaction, DRTransaction, DRTransactionBody, MintTransaction, RevealTransaction,
        TallyTransaction, Transaction, VTTransaction, VTTransactionBody,
    },
    transaction_factory::FeeType,
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

#[derive(Debug, Serialize)]
pub struct CreateDataReqResponse {
    pub transaction_id: String,
    #[serde(
        serialize_with = "into_generic_type::<_, TransactionHelper, _>",
        deserialize_with = "from_generic_type::<_, TransactionHelper, _>"
    )]
    pub transaction: types::Transaction,
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

// Serialization helper

/// Transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
// FIXME(#649): Remove clippy skip error
#[allow(clippy::large_enum_variant)]
pub enum TransactionHelper {
    #[serde(
        serialize_with = "into_generic_type::<_, VTTransactionHelper, _>",
        deserialize_with = "from_generic_type::<_, VTTransactionHelper, _>"
    )]
    ValueTransfer(VTTransaction),
    #[serde(
        serialize_with = "into_generic_type::<_, DRTransactionHelper, _>",
        deserialize_with = "from_generic_type::<_, DRTransactionHelper, _>"
    )]
    DataRequest(DRTransaction),
    Commit(CommitTransaction),
    Reveal(RevealTransaction),
    Tally(TallyTransaction),
    Mint(MintTransaction),
}

impl From<Transaction> for TransactionHelper {
    fn from(x: Transaction) -> Self {
        match x {
            Transaction::ValueTransfer(vttransaction) => {
                TransactionHelper::ValueTransfer(vttransaction)
            }
            Transaction::DataRequest(drtransaction) => {
                TransactionHelper::DataRequest(drtransaction)
            }
            Transaction::Commit(committransaction) => TransactionHelper::Commit(committransaction),
            Transaction::Reveal(revealtransaction) => TransactionHelper::Reveal(revealtransaction),
            Transaction::Tally(tallytransaction) => TransactionHelper::Tally(tallytransaction),
            Transaction::Mint(minttransaction) => TransactionHelper::Mint(minttransaction),
        }
    }
}

impl From<TransactionHelper> for Transaction {
    fn from(x: TransactionHelper) -> Self {
        match x {
            TransactionHelper::ValueTransfer(vttransaction) => {
                Transaction::ValueTransfer(vttransaction)
            }
            TransactionHelper::DataRequest(drtransaction) => {
                Transaction::DataRequest(drtransaction)
            }
            TransactionHelper::Commit(committransaction) => Transaction::Commit(committransaction),
            TransactionHelper::Reveal(revealtransaction) => Transaction::Reveal(revealtransaction),
            TransactionHelper::Tally(tallytransaction) => Transaction::Tally(tallytransaction),
            TransactionHelper::Mint(minttransaction) => Transaction::Mint(minttransaction),
        }
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct DRTransactionHelper {
    #[serde(
        serialize_with = "into_generic_type::<_, DRTransactionBodyHelper, _>",
        deserialize_with = "from_generic_type::<_, DRTransactionBodyHelper, _>"
    )]
    pub body: DRTransactionBody,
    pub signatures: Vec<KeyedSignature>,
}

impl From<DRTransaction> for DRTransactionHelper {
    fn from(x: DRTransaction) -> Self {
        DRTransactionHelper {
            body: x.body,
            signatures: x.signatures,
        }
    }
}

impl From<DRTransactionHelper> for DRTransaction {
    fn from(x: DRTransactionHelper) -> Self {
        DRTransaction {
            body: x.body,
            signatures: x.signatures,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct DRTransactionBodyHelper {
    pub inputs: Vec<Input>,
    #[serde(
        serialize_with = "into_generic_type_vec::<_, ValueTransferOutputHelper, _>",
        deserialize_with = "from_generic_type_vec::<_, ValueTransferOutputHelper, _>"
    )]
    pub outputs: Vec<ValueTransferOutput>,
    #[serde(
        serialize_with = "into_generic_type::<_, DataRequestOutputHelper, _>",
        deserialize_with = "from_generic_type::<_, DataRequestOutputHelper, _>"
    )]
    pub dr_output: DataRequestOutput,
}

impl From<DRTransactionBody> for DRTransactionBodyHelper {
    fn from(x: DRTransactionBody) -> Self {
        DRTransactionBodyHelper {
            inputs: x.inputs,
            outputs: x.outputs,
            dr_output: x.dr_output,
        }
    }
}

impl From<DRTransactionBodyHelper> for DRTransactionBody {
    fn from(x: DRTransactionBodyHelper) -> Self {
        DRTransactionBody::new(x.inputs, x.outputs, x.dr_output)
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, Hash, Default)]
struct DataRequestOutputHelper {
    pub data_request: RADRequest,
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub witness_reward: u64,
    #[serde(
        serialize_with = "u16_to_string",
        deserialize_with = "number_from_string"
    )]
    pub witnesses: u16,
    // This fee will be earn by the miner when include commits and/or reveals in the block
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub commit_and_reveal_fee: u64,
    // This field must be >50 and <100.
    // >50 because simple majority
    // <100 because a 100% consensus encourages to commit a RadError for free
    #[serde(
        serialize_with = "u32_to_string",
        deserialize_with = "number_from_string"
    )]
    pub min_consensus_percentage: u32,
    // This field must be >= collateral_minimum, or zero
    // If zero, it will be treated as collateral_minimum
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
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
        DataRequestOutput {
            data_request: x.data_request,
            witness_reward: x.witness_reward,
            witnesses: x.witnesses,
            commit_and_reveal_fee: x.commit_and_reveal_fee,
            min_consensus_percentage: x.min_consensus_percentage,
            collateral: x.collateral,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct VTTransactionHelper {
    #[serde(
        serialize_with = "into_generic_type::<_, VTTransactionBodyHelper, _>",
        deserialize_with = "from_generic_type::<_, VTTransactionBodyHelper, _>"
    )]
    pub body: VTTransactionBody,
    pub signatures: Vec<KeyedSignature>,
}

impl From<VTTransaction> for VTTransactionHelper {
    fn from(x: VTTransaction) -> Self {
        VTTransactionHelper {
            body: x.body,
            signatures: x.signatures,
        }
    }
}

impl From<VTTransactionHelper> for VTTransaction {
    fn from(x: VTTransactionHelper) -> Self {
        VTTransaction {
            body: x.body,
            signatures: x.signatures,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct VTTransactionBodyHelper {
    pub inputs: Vec<Input>,
    #[serde(
        serialize_with = "into_generic_type_vec::<_, ValueTransferOutputHelper, _>",
        deserialize_with = "from_generic_type_vec::<_, ValueTransferOutputHelper, _>"
    )]
    pub outputs: Vec<ValueTransferOutput>,
}

impl From<VTTransactionBody> for VTTransactionBodyHelper {
    fn from(x: VTTransactionBody) -> Self {
        VTTransactionBodyHelper {
            inputs: x.inputs,
            outputs: x.outputs,
        }
    }
}

impl From<VTTransactionBodyHelper> for VTTransactionBody {
    fn from(x: VTTransactionBodyHelper) -> Self {
        VTTransactionBody::new(x.inputs, x.outputs)
    }
}

/// Value transfer output transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, Hash)]
pub struct ValueTransferOutputHelper {
    pub pkh: PublicKeyHash,
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub value: u64,
    /// The value attached to a time-locked output cannot be spent before the specified
    /// timestamp. That is, they cannot be used as an input in any transaction of a
    /// subsequent block proposed for an epoch whose opening timestamp predates the time lock.
    pub time_lock: u64,
}

impl From<ValueTransferOutput> for ValueTransferOutputHelper {
    fn from(x: ValueTransferOutput) -> Self {
        ValueTransferOutputHelper {
            pkh: x.pkh,
            value: x.value,
            time_lock: x.time_lock,
        }
    }
}

impl From<ValueTransferOutputHelper> for ValueTransferOutput {
    fn from(x: ValueTransferOutputHelper) -> Self {
        ValueTransferOutput {
            pkh: x.pkh,
            value: x.value,
            time_lock: x.time_lock,
        }
    }
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
    U: Serialize,
{
    let x = U::from(val.clone());
    x.serialize(serializer)
}

fn from_generic_type_vec<'de, D, T, U>(deserializer: D) -> Result<Vec<U>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
    U: From<T>,
{
    Ok(Vec::<T>::deserialize(deserializer)?
        .into_iter()
        .map(|x| x.into())
        .collect())
}

fn into_generic_type_vec<S, U, T>(val: &[T], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: Clone,
    U: From<T>,
    U: Serialize,
{
    let x: Vec<U> = val.iter().map(|x| x.clone().into()).collect();
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
