use failure::Fail;

use witnet_crypto as crypto;

use crate::{db, repository};

#[derive(Debug, Fail)]
#[fail(display = "error")]
pub enum Error {
    #[fail(display = "rad request failed: {}", _0)]
    Rad(#[cause] witnet_rad::error::RadError),
    #[fail(display = "{}", _0)]
    Mailbox(#[cause] actix::MailboxError),
    #[fail(display = "master key generation failed: {}", _0)]
    MasterKeyGen(#[cause] crypto::key::MasterKeyGenError),
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
}

impl From<crypto::key::MasterKeyGenError> for Error {
    fn from(err: crypto::key::MasterKeyGenError) -> Self {
        Error::MasterKeyGen(err)
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
