use crate::error::RadError;

use crypto::{digest::Digest, sha2};

pub fn sha2_256(input: &[u8]) -> Result<Vec<u8>, RadError> {
    let mut hash_function = sha2::Sha256::new();
    hash_function.input(input);
    let mut digest = [0; 32];
    hash_function.result(&mut digest);

    Ok(digest.to_vec())
}

#[test]
fn test_sha2_256() {
    let input = [72, 101, 108, 108, 111, 44, 32, 87, 111, 114, 108, 100, 33];
    let output_vec = sha2_256(&input).unwrap();
    let output_slice = output_vec.as_slice();
    let expected = &[
        223, 253, 96, 33, 187, 43, 213, 176, 175, 103, 98, 144, 128, 158, 195, 165, 49, 145, 221,
        129, 199, 247, 10, 75, 40, 104, 138, 54, 33, 130, 152, 111,
    ];

    assert_eq!(output_slice, expected);
}
