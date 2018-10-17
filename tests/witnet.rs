use witnet_crypto as crypto;

#[test]
fn greetings() {
    assert_eq!(crypto::greetings(), String::from("Hello from crypto!"));
}
