extern crate witnet_storage as storage;

use storage::greetings;

#[test]
fn storage_greeeting() {
    assert_eq!(greetings(), String::from("Hello from storage!"));
}
