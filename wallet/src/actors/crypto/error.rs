use failure::Fail;

use witnet_crypto::key;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "wrong mnemonic: {}", _0)]
    WrongMnemonic(#[cause] failure::Error),
    #[fail(display = "master key generation failed: {}", _0)]
    KeyGenFailed(#[cause] key::MasterKeyGenError),
}
