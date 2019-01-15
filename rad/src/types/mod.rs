use crate::error::*;
use crate::types::mixed::RadonMixed;
use crate::types::string::RadonString;

use std::fmt;
use witnet_crypto::hash::calculate_sha256;
use witnet_data_structures::{
    chain::Hash,
    serializers::decoders::{TryFrom, TryInto},
};

pub mod mixed;
pub mod string;

pub trait RadonType<'a, T>:
    fmt::Display + From<T> + PartialEq + TryFrom<&'a [u8]> + TryInto<&'a [u8]>
where
    T: fmt::Debug,
{
    fn value(&self) -> T;

    fn hash(self) -> RadResult<Hash> {
        self.try_into()
            .map(calculate_sha256)
            .map(Hash::from)
            .map_err(|_| {
                WitnetError::from(RadError::new(
                    RadErrorKind::Hash,
                    String::from("Failed to hash RADON value or structure"),
                ))
            })
    }
}

#[derive(Debug, PartialEq)]
pub enum RadonTypes<'a> {
    Mixed(RadonMixed),
    String(RadonString<'a>),
}

impl<'a> From<RadonMixed> for RadonTypes<'a> {
    fn from(mixed: RadonMixed) -> Self {
        RadonTypes::Mixed(mixed)
    }
}

impl<'a> From<RadonString<'a>> for RadonTypes<'a> {
    fn from(string: RadonString<'a>) -> Self {
        RadonTypes::String(string)
    }
}
