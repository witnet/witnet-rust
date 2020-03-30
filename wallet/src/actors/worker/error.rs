use failure::Fail;

use crate::{crypto, db, repository};
use witnet_data_structures::chain::Hash;
use witnet_net::client::tcp;

#[derive(Debug, Fail)]
#[fail(display = "error")]
pub enum Error {
    #[fail(display = "rad request failed: {}", _0)]
    Rad(#[cause] witnet_rad::error::RadError),
    #[fail(display = "{}", _0)]
    Mailbox(#[cause] actix::MailboxError),
    #[fail(display = "master key generation failed: {}", _0)]
    KeyGen(#[cause] crypto::Error),
    #[fail(display = "repository failed: {}", _0)]
    Repository(#[cause] repository::Error),
    #[fail(display = "{}", _0)]
    Failure(#[cause] failure::Error),
    #[fail(display = "db error {}", _0)]
    Db(#[cause] db::Error),
    #[fail(display = "wrong wallet database password")]
    WrongPassword,
    #[fail(display = "wallet not found")]
    WalletNotFound,
    #[fail(display = "send error: {}", _0)]
    Send(#[cause] futures::sync::mpsc::SendError<std::string::String>),
    #[fail(display = "node error: {}", _0)]
    Node(#[cause] failure::Error),
    #[fail(display = "error processing a block: {}", _0)]
    Block(#[cause] failure::Error),
}

#[derive(Debug, Fail)]
#[fail(display = "error")]
pub enum BlockError {
    #[fail(
        display = "block is not connected to our local tip of the chain ({} != {})",
        block_previous_beacon, local_chain_tip
    )]
    NotConnectedToLocalChainTip {
        block_previous_beacon: Hash,
        local_chain_tip: Hash,
    },
}

/// Helper function to simplify .map_err on node errors.
pub fn node_error<T: Fail>(err: T) -> Error {
    Error::Node(failure::Error::from(err))
}

/// Helper function to simplify .map_err on block errors.
pub fn block_error<T: Fail>(err: T) -> Error {
    Error::Block(failure::Error::from(err))
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

impl From<failure::Error> for Error {
    fn from(err: failure::Error) -> Self {
        Error::Failure(err)
    }
}

impl From<db::Error> for Error {
    fn from(err: db::Error) -> Self {
        Error::Db(err)
    }
}

impl From<futures::sync::mpsc::SendError<std::string::String>> for Error {
    fn from(err: futures::sync::mpsc::SendError<std::string::String>) -> Self {
        Error::Send(err)
    }
}

impl From<tcp::Error> for Error {
    fn from(err: tcp::Error) -> Self {
        node_error(err)
    }
}

impl From<witnet_data_structures::chain::HashParseError> for Error {
    fn from(err: witnet_data_structures::chain::HashParseError) -> Self {
        block_error(err)
    }
}
