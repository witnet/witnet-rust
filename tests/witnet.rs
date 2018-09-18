extern crate witnet_core as core;
extern crate witnet_crypto as crypto;
extern crate witnet_data_structures as data_structures;
extern crate witnet_p2p as p2p;
extern crate witnet_storage as storage;

#[test]
fn greetings() {
  assert_eq!(core::greetings(), String::from("Hello from core!"));
  assert_eq!(crypto::greetings(), String::from("Hello from crypto!"));
  assert_eq!(data_structures::greetings(), String::from("Hello from data structures!"));
  assert_eq!(p2p::greetings(), String::from("Hello from p2p!"));
  assert_eq!(storage::greetings(), String::from("Hello from storage!"));
}
