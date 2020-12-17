use actix::prelude::*;
use core::fmt::Display;
use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    actors::app,
    types::{self, FromStr, Hashable as _, ProtobufConvert as _},
};

use witnet_data_structures::{
    chain::{Environment, Input, KeyedSignature, PublicKeyHash, ValueTransferOutput},
    transaction::{
        CommitTransaction, DRTransaction, MintTransaction, RevealTransaction, TallyTransaction,
        Transaction, VTTransaction, VTTransactionBody,
    },
    transaction_factory::FeeType,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VttOutputParams {
    pub address: String,
    pub amount: u64,
    pub time_lock: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateVttRequest {
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    fee: u64,
    label: Option<String>,
    #[serde(
        serialize_with = "into_generic_type_vec::<_, VttOutputParamsHelper, _>",
        deserialize_with = "from_generic_type_vec::<_, VttOutputParamsHelper, _>"
    )]
    outputs: Vec<VttOutputParams>,
    session_id: types::SessionId,
    wallet_id: String,
    fee_type: Option<types::FeeType>,
}

/// Part of CreateVttResponse struct, containing additional data to be displayed in clients
/// (e.g. in a confirmation screen)
#[derive(Debug, Serialize)]
pub struct VttMetadata {
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    fee: u64,
    #[serde(
        serialize_with = "into_generic_type_vec::<_, VttOutputParamsHelper, _>",
        deserialize_with = "from_generic_type_vec::<_, VttOutputParamsHelper, _>"
    )]
    outputs: Vec<VttOutputParams>,
}

#[derive(Debug, Serialize)]
pub struct CreateVttResponse {
    pub transaction_id: String,
    #[serde(
        serialize_with = "into_generic_type::<_, TransactionHelper, _>",
        deserialize_with = "from_generic_type::<_, TransactionHelper, _>"
    )]
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

        let fee_type = msg.fee_type.unwrap_or(FeeType::Weighted);

        let f = fut::result(validated).and_then(move |outputs, act: &mut Self, _ctx| {
            let params = types::VttParams {
                fee: msg.fee,
                outputs,
                fee_type,
            };

            act.create_vtt(&msg.session_id, &msg.wallet_id, params)
                .map(move |transaction, _, _| {
                    let fee = match fee_type {
                        FeeType::Absolute => msg.fee,
                        FeeType::Weighted => msg.fee * u64::from(transaction.weight()),
                    };

                    let transaction_id = hex::encode(transaction.hash().as_ref());
                    let bytes = hex::encode(transaction.to_pb_bytes().unwrap());

                    CreateVttResponse {
                        transaction_id,
                        transaction,
                        bytes,
                        metadata: VttMetadata {
                            fee,
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
pub fn validate_output_addresses(
    testnet: bool,
    outputs: &[VttOutputParams],
) -> Result<Vec<types::ValueTransferOutput>, app::ValidationErrors> {
    let environment = if testnet {
        Environment::Testnet
    } else {
        Environment::Mainnet
    };
    outputs.iter().try_fold(vec![], |mut acc, output| {
        PublicKeyHash::from_bech32(environment, &output.address)
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

// Serialization helper

/// Value transfer output transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, Hash, Default)]
pub struct VttOutputParamsHelper {
    pub address: String,
    #[serde(
        serialize_with = "u64_to_string",
        deserialize_with = "number_from_string"
    )]
    pub amount: u64,
    pub time_lock: Option<u64>,
}

impl From<VttOutputParams> for VttOutputParamsHelper {
    fn from(x: VttOutputParams) -> Self {
        VttOutputParamsHelper {
            address: x.address,
            amount: x.amount,
            time_lock: x.time_lock,
        }
    }
}

impl From<VttOutputParamsHelper> for VttOutputParams {
    fn from(x: VttOutputParamsHelper) -> Self {
        VttOutputParams {
            address: x.address,
            amount: x.amount,
            time_lock: x.time_lock,
        }
    }
}

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

/// Value transfer output transaction data structure
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, Hash, Default)]
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

#[cfg(test)]
mod tests {
    use crate::actors::app::{validate_output_addresses, VttOutputParams};
    use witnet_data_structures::chain::{Environment, PublicKeyHash, ValueTransferOutput};

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
