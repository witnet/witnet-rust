use witnet_core as core;
use witnet_crypto as crypto;
use witnet_p2p as p2p;

#[test]
fn greetings() {
  assert_eq!(core::greetings(), String::from("Hello from core!"));
  assert_eq!(crypto::greetings(), String::from("Hello from crypto!"));
  assert_eq!(p2p::greetings(), String::from("Hello from p2p!"));
}
