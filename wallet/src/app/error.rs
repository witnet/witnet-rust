//! # Error type for the App actor.
use failure::Fail;

use witnet_net::client::tcp;
use witnet_rad::error::RadError;

use crate::{crypto, storage};

/// Error type for errors that may originate in the Storage actor.
#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "There is no node connected to the Wallet")]
    NodeNotConnected,
    #[fail(display = "Could not send request. Actor not running.")]
    RequestFailedToSend(#[cause] actix::MailboxError),
    #[fail(display = "Request failed with an error: {}", _0)]
    RequestFailed(#[cause] tcp::Error),
    #[fail(display = "Could not subscribe: {}", _0)]
    SubscribeFailed(&'static str),
    #[fail(display = "Could not unsubscribe: {}", _0)]
    UnsubscribeFailed(&'static str),
    #[fail(display = "Could not run RAD request. Actor not running.")]
    RadScheduleFailed(#[cause] actix::MailboxError),
    #[fail(display = "RAD engine failed with: {}", _0)]
    RadFailed(#[cause] RadError),
    #[fail(display = "Could not communicate with database. Actor not running.")]
    StorageFailed(#[cause] actix::MailboxError),
    #[fail(display = "Storage error: {}", _0)]
    Storage(#[cause] storage::Error),
    #[fail(display = "Could not communicate with cryptographic engine. Actor not running.")]
    CryptoFailed(#[cause] actix::MailboxError),
    #[fail(display = "Crypto error: {}", _0)]
    Crypto(#[cause] crypto::Error),
}
