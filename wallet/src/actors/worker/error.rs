use failure::Fail;

use witnet_crypto as crypto;

#[derive(Debug, Fail)]
#[fail(display = "error")]
pub enum Error {
    #[fail(display = "cannot decrypt, invalid data length")]
    InvalidDataLen,
    #[fail(display = "rad request failed: {}", _0)]
    Rad(#[cause] witnet_rad::error::RadError),
    #[fail(display = "db key {:?} not found", _0)]
    DbKeyNotFound(String),
    #[fail(display = "{}", _0)]
    Mailbox(#[cause] actix::MailboxError),
    #[fail(display = "rocksdb error: {}", _0)]
    Db(#[cause] rocksdb::Error),
    #[fail(display = "master key generation failed: {}", _0)]
    MasterKeyGen(#[cause] crypto::key::MasterKeyGenError),
    #[fail(display = "key derivation failed: {}", _0)]
    KeyDerivation(#[cause] crypto::key::KeyDerivationError),
    #[fail(display = "bincode failed: {}", _0)]
    Bincode(#[cause] bincode::Error),
    #[fail(display = "cipher failed: {}", _0)]
    Cipher(#[cause] crypto::cipher::Error),
}

impl From<rocksdb::Error> for Error {
    fn from(err: rocksdb::Error) -> Self {
        Error::Db(err)
    }
}

impl From<crypto::key::MasterKeyGenError> for Error {
    fn from(err: crypto::key::MasterKeyGenError) -> Self {
        Error::MasterKeyGen(err)
    }
}

impl From<crypto::key::KeyDerivationError> for Error {
    fn from(err: crypto::key::KeyDerivationError) -> Self {
        Error::KeyDerivation(err)
    }
}

impl From<bincode::Error> for Error {
    fn from(err: bincode::Error) -> Self {
        Error::Bincode(err)
    }
}

impl From<crypto::cipher::Error> for Error {
    fn from(err: crypto::cipher::Error) -> Self {
        Error::Cipher(err)
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
