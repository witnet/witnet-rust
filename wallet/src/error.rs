use failure::Fail;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "{}", _0)]
    Internal(#[cause] failure::Error),
}

pub fn internal<E: Fail>(err: E) -> Error {
    Error::Internal(failure::Error::from(err))
}

impl From<diesel::result::Error> for Error {
    fn from(err: diesel::result::Error) -> Self {
        internal(err)
    }
}
