use failure::Fail;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "wrong mnemonic: {}", _0)]
    WrongMnemonic(#[cause] failure::Error),
}
