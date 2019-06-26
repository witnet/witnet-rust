use failure::Fail;

use witnet_crypto::key;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "Master Key generation failed: {}", _0)]
    KeyGenFailed(#[cause] key::MasterKeyGenError),
}
