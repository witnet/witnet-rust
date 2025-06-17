use thiserror::Error;

use crate::{crypto, db, repository};
use witnet_data_structures::chain::Hash;
use witnet_net::client::tcp;

#[derive(Debug, Error)]
#[error("error")]
pub enum Error {
    #[error("rad request failed: {0}")]
    Rad(witnet_rad::error::RadError),
    #[error("{0}")]
    Mailbox(actix::MailboxError),
    #[error("master key generation failed: {0}")]
    KeyGen(crypto::Error),
    #[error("repository failed: {0}")]
    Repository(repository::Error),
    #[error("{0}")]
    Failure(anyhow::Error),
    #[error("db error {0}")]
    Db(db::Error),
    #[error("wrong wallet database password")]
    WrongPassword,
    #[error("wallet not found")]
    WalletNotFound,
    #[error("send error: {0}")]
    Send(futures01::sync::mpsc::SendError<std::string::String>),
    #[error("node error: {0}")]
    Node(anyhow::Error),
    #[error("JsonRPC timeout error")]
    JsonRpcTimeout,
    #[error("error processing a block: {0}")]
    Block(anyhow::Error),
    #[error("output ({0}) not found in transaction: {1}")]
    OutputIndexNotFound(u32, String),
    #[error("transaction type not supported")]
    TransactionTypeNotSupported,
    #[error("epoch calculation error {0}")]
    EpochCalculation(witnet_data_structures::error::EpochCalculationError),
    #[error("wallet already exists: {0}")]
    WalletAlreadyExists(String),
    #[error("error while syncing: node is behind our local tip (#{0} < #{1})")]
    NodeBehindLocalTip(u32, u32),
    #[error("the provided `birth_date` epoch is greater than the current epoch({0} > {1})")]
    InvalidBirthDate(u32, u32),
}

#[derive(Debug, Error)]
#[error("error")]
pub enum BlockError {
    #[error(
        "block is not connected to our local tip of the chain ({block_previous_beacon} != {local_chain_tip})"
    )]
    NotConnectedToLocalChainTip {
        block_previous_beacon: Hash,
        local_chain_tip: Hash,
    },
}

/// Helper function to simplify .map_err on node errors.
pub fn node_error<T: std::error::Error + Send + Sync + 'static>(err: T) -> Error {
    Error::Node(anyhow::Error::from(err))
}

/// Helper function to simplify .map_err on timeout errors.
pub fn jsonrpc_timeout_error<T: std::error::Error + 'static>(_err: T) -> Error {
    Error::JsonRpcTimeout
}

/// Helper function to simplify .map_err on block errors.
pub fn block_error<T: std::error::Error + Send + Sync + 'static>(err: T) -> Error {
    Error::Block(anyhow::Error::from(err))
}

impl From<crypto::Error> for Error {
    fn from(err: crypto::Error) -> Self {
        Self::KeyGen(err)
    }
}

impl From<actix::MailboxError> for Error {
    fn from(err: actix::MailboxError) -> Self {
        Error::Mailbox(err)
    }
}

impl From<witnet_rad::error::RadError> for Error {
    fn from(err: witnet_rad::error::RadError) -> Self {
        Error::Rad(err)
    }
}

impl From<repository::Error> for Error {
    fn from(err: repository::Error) -> Self {
        Error::Repository(err)
    }
}

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error::Failure(err)
    }
}

impl From<db::Error> for Error {
    fn from(err: db::Error) -> Self {
        Error::Db(err)
    }
}

impl From<futures01::sync::mpsc::SendError<std::string::String>> for Error {
    fn from(err: futures01::sync::mpsc::SendError<std::string::String>) -> Self {
        Error::Send(err)
    }
}

impl From<tcp::Error> for Error {
    fn from(err: tcp::Error) -> Self {
        match err {
            tcp::Error::RequestTimedOut(_) => jsonrpc_timeout_error(err),
            _ => node_error(err),
        }
    }
}

impl From<witnet_data_structures::chain::HashParseError> for Error {
    fn from(err: witnet_data_structures::chain::HashParseError) -> Self {
        block_error(err)
    }
}

impl From<witnet_data_structures::error::EpochCalculationError> for Error {
    fn from(err: witnet_data_structures::error::EpochCalculationError) -> Self {
        Error::EpochCalculation(err)
    }
}
