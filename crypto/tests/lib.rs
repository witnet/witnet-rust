extern crate witnet_crypto as crypto;

use crate::crypto::greetings;

#[test]
fn crypto_greeeting() {
    assert_eq!(greetings(), String::from("Hello from crypto!"));
}
