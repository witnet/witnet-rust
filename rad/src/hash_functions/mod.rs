// FIXME: https://github.com/rust-num/num-derive/issues/20
#![allow(clippy::useless_attribute)]

mod sha2;

use crate::error::*;
use crate::hash_functions::sha2::sha2_256;

use num_derive::FromPrimitive;
use std::fmt;

#[derive(Debug, FromPrimitive, PartialEq)]
pub enum RadonHashFunctions {
    Fail = -1,
    Blake256 = 0x00,
    Blake512 = 0x01,
    Blake2s256 = 0x02,
    Blake2b512 = 0x03,
    MD5_128 = 0x04,
    Ripemd128 = 0x05,
    Ripemd160 = 0x06,
    Ripemd320 = 0x07,
    SHA1_160 = 0x08,
    SHA2_224 = 0x09,
    SHA2_256 = 0x0A,
    SHA2_384 = 0x0B,
    SHA2_512 = 0x0C,
    SHA3_224 = 0x0D,
    SHA3_256 = 0x0E,
    SHA3_384 = 0x0F,
    SHA3_512 = 0x10,
    Whirlpool512 = 0x11,
}

impl fmt::Display for RadonHashFunctions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RadonHashFunctions::{:?}", self)
    }
}

pub fn hash(input: &[u8], hash_function_code: RadonHashFunctions) -> RadResult<Vec<u8>> {
    match hash_function_code {
        RadonHashFunctions::SHA2_256 => sha2_256(input),
        _ => Err(WitnetError::from(RadError::new(
            RadErrorKind::UnsupportedHashFunction,
            format!(
                "HashFunction {:} is not yet implemented",
                hash_function_code
            ),
        ))),
    }
}

#[test]
fn test_hash() {
    let input = [72, 101, 108, 108, 111, 44, 32, 87, 111, 114, 108, 100, 33];
    let output_vec = hash(&input, RadonHashFunctions::SHA2_256).unwrap();
    let output_slice = output_vec.as_slice();
    let expected = &[
        223, 253, 96, 33, 187, 43, 213, 176, 175, 103, 98, 144, 128, 158, 195, 165, 49, 145, 221,
        129, 199, 247, 10, 75, 40, 104, 138, 54, 33, 130, 152, 111,
    ];

    assert_eq!(output_slice, expected);
}
