use thiserror::Error;

use crate::{crypto, db};
use witnet_crypto::key::KeyDerivationError;
use witnet_data_structures::{
    chain::{DataRequestOutput, HashParseError, PublicKeyHashParseError},
    error::TransactionError,
};

#[derive(Debug, Error)]
#[error("Database Error")]
pub enum Error {
    #[error("maximum key index reached for account")]
    IndexOverflow,
    #[error("overflow when calculating transaction value")]
    TransactionValueOverflow,
    #[error("transaction balance overflowed")]
    TransactionBalanceOverflow,
    #[error("transaction balance underflowed")]
    TransactionBalanceUnderflow,
    #[error("Invalid PKH: {0}")]
    Pkh(PublicKeyHashParseError),
    #[error(
        "Wallet account has not enough balance: total {total_balance}, available {available_balance}, transaction value {transaction_value}"
    )]
    InsufficientBalance {
        total_balance: u64,
        available_balance: u64,
        transaction_value: u64,
    },
    #[error("maximum transaction id reached for account")]
    TransactionIdOverflow,
    #[error("mutex poison error")]
    MutexPoison,
    #[error("database failed: {0}")]
    Db(db::Error),
    #[error("cipher failed {0}")]
    Cipher(witnet_crypto::cipher::Error),
    #[error("{0}")]
    Failure(anyhow::Error),
    #[error("key derivation failed: {0}")]
    KeyDerivation(KeyDerivationError),
    #[error("transaction type not supported: {0}")]
    UnsupportedTransactionType(String),
    #[error("tally decode failed: {0}")]
    TallyRadDecode(String),
    #[error("reveal decode failed: {0}")]
    RevealRadDecode(String),
    #[error("transaction metadata type is wrong: {0}")]
    WrongMetadataType(String),
    #[error("block consolidation failed: {0}")]
    BlockConsolidation(String),
    #[error("hash parsing failed: {0}")]
    HashParse(HashParseError),
    #[error("failed creating a transaction: {0}")]
    TransactionCreation(TransactionError),
    #[error("Bech32 serialization error: {0}")]
    Bech32(bech32::Error),
    #[error("Crypto operation failed: {0}")]
    Crypto(crypto::Error),
    #[error("Master key serialization failed")]
    KeySerialization,
    #[error("failed because wallet is still syncing: {0}")]
    StillSyncing(String),
    #[error("Weight limit reached when trying to create a VTT of value {0} nanoWits")]
    MaximumVTTWeightReached(u64),
    #[error("Weight limit reached when trying to create a Data Request. \n > {0:?}")]
    MaximumDRWeightReached(Box<DataRequestOutput>),
    #[error("The chosen fee seems too large")]
    FeeTooLarge,
    #[error("Unknown Fee Type specified")]
    UnknownFeeType,
    #[error("Wallet not found")]
    WalletNotFound,
    #[error("Secp256k1 error: {0}")]
    Secp256k1(witnet_crypto::secp256k1::Error),
}

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error::Failure(err)
    }
}

impl From<witnet_crypto::cipher::Error> for Error {
    fn from(err: witnet_crypto::cipher::Error) -> Self {
        Error::Cipher(err)
    }
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_err: std::sync::PoisonError<T>) -> Self {
        Error::MutexPoison
    }
}

impl From<db::Error> for Error {
    fn from(err: db::Error) -> Self {
        Error::Db(err)
    }
}

impl From<KeyDerivationError> for Error {
    fn from(err: KeyDerivationError) -> Self {
        Error::KeyDerivation(err)
    }
}

impl From<PublicKeyHashParseError> for Error {
    fn from(err: PublicKeyHashParseError) -> Self {
        Error::Pkh(err)
    }
}

impl From<HashParseError> for Error {
    fn from(err: HashParseError) -> Self {
        Error::HashParse(err)
    }
}

impl From<TransactionError> for Error {
    fn from(err: TransactionError) -> Self {
        match err {
            TransactionError::NoMoney {
                total_balance,
                available_balance,
                transaction_value,
            } => Error::InsufficientBalance {
                total_balance,
                available_balance,
                transaction_value,
            },
            TransactionError::OutputValueOverflow => Error::TransactionValueOverflow,
            TransactionError::FeeOverflow => Error::FeeTooLarge,
            TransactionError::ValueTransferWeightLimitExceeded { weight, .. } => {
                Error::MaximumVTTWeightReached(u64::from(weight))
            }
            TransactionError::DataRequestWeightLimitExceeded { dr_output, .. } => {
                Error::MaximumDRWeightReached(dr_output)
            }
            _ => Error::TransactionCreation(err),
        }
    }
}

impl From<witnet_crypto::secp256k1::Error> for Error {
    fn from(err: witnet_crypto::secp256k1::Error) -> Self {
        Error::Secp256k1(err)
    }
}
