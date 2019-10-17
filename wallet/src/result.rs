use crate::error;

pub type Result<T> = std::result::Result<T, error::Error>;
