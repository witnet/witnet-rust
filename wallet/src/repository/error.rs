use failure::Fail;

use crate::{crypto, db};
use witnet_crypto::key::KeyDerivationError;
use witnet_data_structures::{
    chain::{DataRequestOutput, HashParseError, PublicKeyHashParseError},
    error::TransactionError,
};

#[derive(Debug, Fail)]
#[fail(display = "Database Error")]
pub enum Error {
    #[fail(display = "maximum key index reached for account")]
    IndexOverflow,
    #[fail(display = "overflow when calculating transaction value")]
    TransactionValueOverflow,
    #[fail(display = "transaction balance overflowed")]
    TransactionBalanceOverflow,
    #[fail(display = "transaction balance underflowed")]
    TransactionBalanceUnderflow,
    #[fail(display = "Invalid PKH: {}", _0)]
    Pkh(#[cause] PublicKeyHashParseError),
    #[fail(
        display = "Wallet account has not enough balance: total {}, available {}, transaction value {}",
        total_balance, available_balance, transaction_value
    )]
    InsufficientBalance {
        total_balance: u64,
        available_balance: u64,
        transaction_value: u64,
    },
    #[fail(display = "maximum transaction id reached for account")]
    TransactionIdOverflow,
    #[fail(display = "mutex poison error")]
    MutexPoison,
    #[fail(display = "database failed: {}", _0)]
    Db(#[cause] db::Error),
    #[fail(display = "cipher failed {}", _0)]
    Cipher(#[cause] witnet_crypto::cipher::Error),
    #[fail(display = "{}", _0)]
    Failure(#[cause] failure::Error),
    #[fail(display = "key derivation failed: {}", _0)]
    KeyDerivation(#[cause] KeyDerivationError),
    #[fail(display = "transaction type not supported: {}", _0)]
    UnsupportedTransactionType(String),
    #[fail(display = "tally decode failed: {}", _0)]
    TallyRadDecode(String),
    #[fail(display = "reveal decode failed: {}", _0)]
    RevealRadDecode(String),
    #[fail(display = "transaction metadata type is wrong: {}", _0)]
    WrongMetadataType(String),
    #[fail(display = "block consolidation failed: {}", _0)]
    BlockConsolidation(String),
    #[fail(display = "hash parsing failed: {}", _0)]
    HashParse(#[cause] HashParseError),
    #[fail(display = "failed creating a transaction: {}", _0)]
    TransactionCreation(#[cause] TransactionError),
    #[fail(display = "Bech32 serialization error: {}", _0)]
    Bech32(#[cause] bech32::Error),
    #[fail(display = "Crypto operation failed: {}", _0)]
    Crypto(#[cause] crypto::Error),
    #[fail(display = "Master key serialization failed")]
    KeySerialization,
    #[fail(display = "failed because wallet is still syncing: {}", _0)]
    StillSyncing(String),
    #[fail(
        display = "Weight limit reached when trying to create a VTT of value {} nanoWits",
        _0
    )]
    MaximumVTTWeightReached(u64),
    #[fail(
        display = "Weight limit reached when trying to create a Data Request. \n > {:?}",
        _0
    )]
    MaximumDRWeightReached(DataRequestOutput),
    #[fail(display = "The chosen fee seems too large")]
    FeeTooLarge,
    #[fail(display = "Unknown Fee Type specified")]
    UnknownFeeType,
    #[fail(display = "Wallet not found")]
    WalletNotFound,
    #[fail(display = "Secp256k1 error: {}", _0)]
    Secp256k1(#[cause] witnet_crypto::secp256k1::Error),
}

impl From<failure::Error> for Error {
    fn from(err: failure::Error) -> Self {
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
